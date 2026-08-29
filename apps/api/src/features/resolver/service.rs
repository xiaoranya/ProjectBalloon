use std::net::IpAddr;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;
use crate::features::auth::model::AuthUser;

use crate::features::resolver::model::{
    AutoPlayRequest, CreateRequest, ResolverEventResponse, ResolverPublicStateResponse,
    ResolverRunResponse, ResolverSourceSnapshotResponse, ResolverSourcesResponse, RunRow,
};
use crate::features::resolver::plan::{build_states, encode_state, load_source_snapshot};

pub struct ResolverService {
    database: PgPool,
}

impl ResolverService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(crate) async fn create(
        &self,
        contest_id: i64,
        request: CreateRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<ResolverRunResponse, AppError> {
        require_operator(actor)?;
        if request.public_snapshot_id <= 0 || request.final_snapshot_id <= 0 {
            return Err(AppError::validation("snapshotId", "must be positive"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin resolver creation", error))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("resolver:{contest_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("lock resolver creation", error))?;
        let contest = sqlx::query_as::<_, (String, Option<OffsetDateTime>)>(
            "SELECT status, freeze_at FROM contests WHERE id = $1 AND deleted_at IS NULL FOR SHARE",
        )
        .bind(contest_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load resolver contest", error))?
        .ok_or_else(resolver_not_found)?;
        if request.official && !matches!(contest.0.as_str(), "ENDED" | "ARCHIVED") {
            return Err(AppError::conflict(
                "RESOLVER_CONTEST_NOT_FINAL",
                "An official Resolver run requires an ended or archived contest",
            ));
        }
        let public = load_source_snapshot(&mut tx, request.public_snapshot_id).await?;
        let final_snapshot = load_source_snapshot(&mut tx, request.final_snapshot_id).await?;
        if public.contest_id != contest_id
            || final_snapshot.contest_id != contest_id
            || public.variant != "PUBLIC"
            || !public.frozen
            || final_snapshot.variant != "ADMIN"
        {
            return Err(AppError::validation(
                "snapshots",
                "must be a frozen PUBLIC snapshot and an ADMIN snapshot from this contest",
            ));
        }
        let states = build_states(public.board, final_snapshot.board)?;
        let total_steps = i32::try_from(states.len().saturating_sub(1))
            .map_err(|error| AppError::internal("count resolver steps", error))?;
        let encoded_states = states.iter().map(encode_state).collect::<Result<Vec<_>, _>>()?;
        let mut plan_hasher = Sha256::new();
        plan_hasher.update(public.sha256.as_bytes());
        plan_hasher.update(final_snapshot.sha256.as_bytes());
        for (state, _) in &encoded_states {
            plan_hasher.update(state.as_bytes());
        }
        let plan_sha256 = hex::encode(plan_hasher.finalize());
        let run_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO resolver_runs
                (contest_id, official, status, current_step, total_steps,
                 source_public_snapshot_id, source_final_snapshot_id, plan_sha256,
                 created_by_user_id)
            VALUES ($1, $2, 'READY', 0, $3, $4, $5, $6, $7) RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(request.official)
        .bind(total_steps)
        .bind(request.public_snapshot_id)
        .bind(request.final_snapshot_id)
        .bind(&plan_sha256)
        .bind(actor.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_create_error)?;
        for (index, (state_data, state_sha256)) in encoded_states.iter().enumerate() {
            let step = i32::try_from(index)
                .map_err(|error| AppError::internal("convert resolver step", error))?;
            sqlx::query("INSERT INTO resolver_snapshots (run_id, step_index, state_data, state_sha256) VALUES ($1, $2, $3, $4)")
                .bind(run_id).bind(step).bind(state_data).bind(state_sha256)
                .execute(&mut *tx).await
                .map_err(|error| AppError::internal("persist resolver snapshot", error))?;
        }
        let (initial_data, initial_sha) = &encoded_states[0];
        sqlx::query("INSERT INTO resolver_current_state (run_id, step_index, state_data, state_sha256) VALUES ($1, 0, $2, $3)")
            .bind(run_id).bind(initial_data).bind(initial_sha).execute(&mut *tx).await
            .map_err(|error| AppError::internal("persist resolver current state", error))?;
        if let Some(freeze_at) = contest.1 {
            sqlx::query(
                r#"
                INSERT INTO resolver_pending_submissions
                    (run_id, submission_id, team_id, problem_id, submitted_at, verdict_at_snapshot)
                SELECT $1, id, team_id, problem_id, submitted_at, status
                FROM submissions WHERE contest_id = $2 AND submitted_at >= $3
                ORDER BY submitted_at, id
                "#,
            )
            .bind(run_id)
            .bind(contest_id)
            .bind(freeze_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("snapshot resolver pending submissions", error))?;
        }
        insert_event(&mut tx, run_id, 0, "CREATED", actor.id, json!({"stepIndex": 0})).await?;
        audit(&mut tx, actor.id, "RESOLVER_CREATED", run_id, ip).await?;
        tx.commit().await.map_err(|error| AppError::internal("commit resolver creation", error))?;
        load_run(&self.database, run_id).await
    }

    pub(crate) async fn get(
        &self,
        id: i64,
        actor: &AuthUser,
    ) -> Result<ResolverRunResponse, AppError> {
        require_operator(actor)?;
        load_run(&self.database, id).await
    }

    pub(crate) async fn list(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ResolverRunResponse>, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest_id).await?;
        sqlx::query_as::<_, RunRow>(safe_sql!(
            "{RESOLVER_RUN_SQL} WHERE run.contest_id = $1 ORDER BY run.official DESC, run.created_at DESC"
        ))
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list resolver runs", error))?
        .into_iter()
        .map(RunRow::response)
        .collect()
    }

    pub(crate) async fn sources(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<ResolverSourcesResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest_id).await?;
        let load = |variant: &'static str, frozen_only: bool| async move {
            sqlx::query_as::<_, ResolverSourceSnapshotResponse>(
                r#"
                SELECT id, version, generated_at, payload_sha256
                FROM scoreboard_snapshots
                WHERE contest_id = $1 AND variant = $2
                  AND group_name IS NULL AND participation_type IS NULL
                  AND (NOT $3 OR frozen)
                ORDER BY version DESC LIMIT 1
                "#,
            )
            .bind(contest_id)
            .bind(variant)
            .bind(frozen_only)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load Resolver source snapshot", error))?
            .ok_or_else(|| {
                AppError::not_found(
                    "RESOLVER_SOURCE_SNAPSHOT_NOT_FOUND",
                    "Required Resolver source snapshot was not found",
                )
            })
        };
        let (public_snapshot, final_snapshot) =
            tokio::try_join!(load("PUBLIC", true), load("ADMIN", false))?;
        Ok(ResolverSourcesResponse { public_snapshot, final_snapshot })
    }

    pub(crate) async fn public_state(
        &self,
        id: i64,
    ) -> Result<ResolverPublicStateResponse, AppError> {
        let run = load_run(&self.database, id).await?;
        let visible = sqlx::query_scalar::<_, bool>(
            "SELECT visibility = 'PUBLIC' FROM contests WHERE id=$1 AND deleted_at IS NULL",
        )
        .bind(run.contest_id)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check Resolver contest visibility", error))?;
        if !visible || !run.official || run.status == "READY" {
            return Err(resolver_not_found());
        }
        Ok(ResolverPublicStateResponse {
            id: run.id,
            contest_id: run.contest_id,
            status: run.status,
            current_step: run.current_step,
            total_steps: run.total_steps,
            updated_at: run.updated_at,
            state: run.state,
        })
    }

    pub(crate) async fn events(
        &self,
        id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ResolverEventResponse>, AppError> {
        require_operator(actor)?;
        sqlx::query_as::<_, EventRow>(
            r#"
            SELECT event.id, event.event_type, event.payload, event.sequence,
                   event.actor_user_id, event.created_at
            FROM resolver_events event
            JOIN resolver_runs run
                ON run.id = event.run_id
            JOIN contests contest
                ON contest.id = run.contest_id AND contest.deleted_at IS NULL
            WHERE event.run_id = $1
            ORDER BY event.sequence
            "#,
        )
        .bind(id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list resolver events", error))?
        .into_iter()
        .map(EventRow::response)
        .collect()
    }

    pub(crate) async fn command(
        &self,
        id: i64,
        action: &'static str,
        expected_version: i32,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<ResolverRunResponse, AppError> {
        require_operator(actor)?;
        if expected_version < 0 {
            return Err(AppError::validation("expectedVersion", "must not be negative"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin resolver command", error))?;
        let (status, step, total, version, official) = sqlx::query_as::<_, (String, i32, i32, i32, bool)>(
            "SELECT run.status, run.current_step, run.total_steps, run.version, run.official FROM resolver_runs run JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL WHERE run.id = $1 FOR UPDATE OF run",
        ).bind(id).fetch_optional(&mut *tx).await
            .map_err(|error| AppError::internal("lock resolver run", error))?
            .ok_or_else(resolver_not_found)?;
        if version != expected_version {
            return Err(AppError::conflict(
                "RESOLVER_VERSION_STALE",
                "Resolver run changed; reload and retry",
            ));
        }
        let (next_status, next_step) = match action {
            "START" if status == "READY" => ("RUNNING", step),
            "NEXT" if status == "RUNNING" && step < total => ("RUNNING", step + 1),
            "PREVIOUS" if matches!(status.as_str(), "RUNNING" | "PAUSED") && step > 0 => {
                (status.as_str(), step - 1)
            }
            "PAUSE" if status == "RUNNING" => ("PAUSED", step),
            "RESUME" if status == "PAUSED" => ("RUNNING", step),
            "COMPLETE" if matches!(status.as_str(), "RUNNING" | "PAUSED") && step == total => {
                ("COMPLETED", step)
            }
            _ => {
                return Err(AppError::conflict(
                    "RESOLVER_STATE_CHANGED",
                    "Resolver command is not valid in the current state",
                ));
            }
        };
        sqlx::query(
            r#"
            UPDATE resolver_runs SET status = $2, current_step = $3,
                started_at = CASE WHEN $2 = 'RUNNING' AND started_at IS NULL THEN now() ELSE started_at END,
                completed_at = CASE WHEN $2 = 'COMPLETED' THEN now() ELSE NULL END,
                auto_play_enabled = CASE WHEN $2 IN ('PAUSED', 'COMPLETED') OR $3 >= total_steps THEN false ELSE auto_play_enabled END,
                next_auto_at = CASE
                    WHEN $2 IN ('PAUSED', 'COMPLETED') THEN NULL
                    WHEN auto_play_enabled AND $3 < total_steps
                    THEN now() + auto_play_interval_ms * interval '1 millisecond'
                    ELSE NULL END,
                updated_at = now(), version = version + 1 WHERE id = $1
            "#,
        ).bind(id).bind(next_status).bind(next_step).execute(&mut *tx).await
            .map_err(|error| AppError::internal("update resolver run", error))?;
        if next_step != step {
            sqlx::query(
                r#"
                UPDATE resolver_current_state current SET step_index = snapshot.step_index,
                    state_data = snapshot.state_data, state_sha256 = snapshot.state_sha256,
                    updated_at = now(), version = current.version + 1
                FROM resolver_snapshots snapshot
                WHERE current.run_id = $1 AND snapshot.run_id = current.run_id
                    AND snapshot.step_index = $2
                "#,
            )
            .bind(id)
            .bind(next_step)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("advance resolver state", error))?;
        }
        let sequence = sqlx::query_scalar::<_, i32>(
            "SELECT coalesce(max(sequence), -1) + 1 FROM resolver_events WHERE run_id = $1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("allocate resolver event", error))?;
        insert_event(
            &mut tx,
            id,
            sequence,
            action,
            actor.id,
            json!({"stepIndex": next_step, "status": next_status}),
        )
        .await?;
        audit(&mut tx, actor.id, &format!("RESOLVER_{action}"), id, ip).await?;
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) SELECT $1, contest_id, 'RESOLVER_STATE_CHANGED', $2, $3 FROM resolver_runs WHERE id = $4")
            .bind(Uuid::new_v4()).bind(if official { "PUBLIC" } else { "STAFF" })
            .bind(json!({"resolverRunId": id, "action": action, "stepIndex": next_step, "status": next_status}))
            .bind(id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("enqueue resolver event", error))?;
        tx.commit().await.map_err(|error| AppError::internal("commit resolver command", error))?;
        load_run(&self.database, id).await
    }

    pub(crate) async fn configure_auto_play(
        &self,
        id: i64,
        request: AutoPlayRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<ResolverRunResponse, AppError> {
        require_operator(actor)?;
        if request.expected_version < 0 || !(500..=60_000).contains(&request.interval_milliseconds)
        {
            return Err(AppError::validation(
                "intervalMilliseconds",
                "must be between 500 and 60000",
            ));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin Resolver auto-play update", error))?;
        let (status, step, total, version, official) = sqlx::query_as::<_, (String, i32, i32, i32, bool)>(
            "SELECT run.status, run.current_step, run.total_steps, run.version, run.official FROM resolver_runs run JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL WHERE run.id = $1 FOR UPDATE OF run",
        ).bind(id).fetch_optional(&mut *tx).await
            .map_err(|error| AppError::internal("lock Resolver auto-play", error))?
            .ok_or_else(resolver_not_found)?;
        if version != request.expected_version {
            return Err(AppError::conflict(
                "RESOLVER_VERSION_STALE",
                "Resolver run changed; reload and retry",
            ));
        }
        if request.enabled && (status != "RUNNING" || step >= total) {
            return Err(AppError::conflict(
                "RESOLVER_AUTO_PLAY_INVALID",
                "Auto-play requires a running Resolver with remaining steps",
            ));
        }
        sqlx::query("UPDATE resolver_runs SET auto_play_enabled = $2, auto_play_interval_ms = $3, next_auto_at = CASE WHEN $2 THEN now() + $3 * interval '1 millisecond' ELSE NULL END, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(request.enabled).bind(request.interval_milliseconds)
            .execute(&mut *tx).await
            .map_err(|error| AppError::internal("configure Resolver auto-play", error))?;
        let sequence = next_event_sequence(&mut tx, id).await?;
        insert_event(&mut tx, id, sequence, "AUTO_PLAY", actor.id,
            json!({"enabled": request.enabled, "intervalMilliseconds": request.interval_milliseconds, "stepIndex": step})).await?;
        audit(&mut tx, actor.id, "RESOLVER_AUTO_PLAY", id, ip).await?;
        enqueue_state_event(&mut tx, id, official, "AUTO_PLAY", step, &status).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit Resolver auto-play", error))?;
        load_run(&self.database, id).await
    }
}
const RESOLVER_RUN_SQL: &str = r#"SELECT run.id, run.contest_id, run.official, run.status,
 run.current_step, run.total_steps, run.source_public_snapshot_id,
 run.source_final_snapshot_id, run.plan_sha256, run.created_by_user_id,
 run.started_at, run.completed_at, run.auto_play_enabled, run.auto_play_interval_ms,
 run.next_auto_at, run.created_at, run.updated_at, run.version,
 current.state_data FROM resolver_runs run JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL JOIN resolver_current_state current ON current.run_id = run.id"#;

async fn require_active_contest(database: &PgPool, contest_id: i64) -> Result<(), AppError> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check resolver contest", error))?;
    if active { Ok(()) } else { Err(resolver_not_found()) }
}

async fn load_run(database: &PgPool, id: i64) -> Result<ResolverRunResponse, AppError> {
    sqlx::query_as::<_, RunRow>(safe_sql!("{RESOLVER_RUN_SQL} WHERE run.id = $1"))
        .bind(id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("load resolver run", error))?
        .ok_or_else(resolver_not_found)?
        .response()
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: i64,
    event_type: String,
    payload: String,
    sequence: i32,
    actor_user_id: Option<i64>,
    created_at: OffsetDateTime,
}

impl EventRow {
    fn response(self) -> Result<ResolverEventResponse, AppError> {
        Ok(ResolverEventResponse {
            id: self.id,
            event_type: self.event_type,
            payload: serde_json::from_str(&self.payload)
                .map_err(|error| AppError::internal("decode resolver event", error))?,
            sequence: self.sequence,
            actor_user_id: self.actor_user_id,
            created_at: self.created_at,
        })
    }
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    sequence: i32,
    event_type: &str,
    actor: i64,
    payload: Value,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO resolver_events (run_id, event_type, payload, sequence, actor_user_id) VALUES ($1, $2, $3, $4, $5)")
        .bind(run_id).bind(event_type).bind(payload.to_string()).bind(sequence).bind(actor)
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("persist resolver event", error))
}

async fn next_event_sequence(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
) -> Result<i32, AppError> {
    sqlx::query_scalar(
        "SELECT coalesce(max(sequence), -1) + 1 FROM resolver_events WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| AppError::internal("allocate Resolver event", error))
}

async fn enqueue_state_event(
    tx: &mut Transaction<'_, Postgres>,
    run_id: i64,
    official: bool,
    action: &str,
    step: i32,
    status: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) SELECT $1, contest_id, 'RESOLVER_STATE_CHANGED', $2, $3 FROM resolver_runs WHERE id = $4")
        .bind(Uuid::new_v4()).bind(if official { "PUBLIC" } else { "STAFF" })
        .bind(json!({"resolverRunId": run_id, "action": action, "stepIndex": step, "status": status}))
        .bind(run_id).execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("enqueue Resolver state event", error))
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'RESOLVER_RUN', $3, $4, 'success')")
        .bind(actor).bind(action).bind(id.to_string()).bind(ip.to_string()).execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("record resolver audit", error))
}

fn require_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::RESOLVER_MANAGE)
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "RESOLVER_PERMISSION_REQUIRED",
            "Resolver management permission is required",
        ))
    }
}

fn map_create_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("uq_resolver_official_run")
    {
        AppError::conflict(
            "RESOLVER_OFFICIAL_EXISTS",
            "This contest already has an official Resolver run",
        )
    } else {
        AppError::internal("create resolver run", error)
    }
}

fn resolver_not_found() -> AppError {
    AppError::not_found("RESOLVER_RUN_NOT_FOUND", "Resolver run was not found")
}
