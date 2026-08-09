use std::{
    cmp::Ordering,
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use tokio::sync::watch;
use tracing::{info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::{
        auth::{AuthContext, model::AuthUser},
        scoreboard::{ScoreboardCell, ScoreboardResponse, ScoreboardRow},
    },
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    public_snapshot_id: i64,
    final_snapshot_id: i64,
    #[serde(default)]
    official: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoPlayRequest {
    expected_version: i32,
    enabled: bool,
    interval_milliseconds: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reveal {
    team_id: i64,
    problem_id: i64,
    before: ScoreboardCell,
    after: ScoreboardCell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolverState {
    step_index: i32,
    total_steps: i32,
    board: ScoreboardResponse,
    last_reveal: Option<Reveal>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverRunResponse {
    id: i64,
    contest_id: i64,
    official: bool,
    status: String,
    current_step: i32,
    total_steps: i32,
    source_public_snapshot_id: i64,
    source_final_snapshot_id: i64,
    plan_sha256: String,
    created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    completed_at: Option<OffsetDateTime>,
    auto_play_enabled: bool,
    auto_play_interval_milliseconds: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    next_auto_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    version: i32,
    state: Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolverPublicStateResponse {
    id: i64,
    contest_id: i64,
    status: String,
    current_step: i32,
    total_steps: i32,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    state: Value,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverEventResponse {
    id: i64,
    event_type: String,
    payload: Value,
    sequence: i32,
    actor_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSourceSnapshotResponse {
    id: i64,
    version: i64,
    #[serde(with = "time::serde::rfc3339")]
    generated_at: OffsetDateTime,
    payload_sha256: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSourcesResponse {
    public_snapshot: ResolverSourceSnapshotResponse,
    final_snapshot: ResolverSourceSnapshotResponse,
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: i64,
    contest_id: i64,
    official: bool,
    status: String,
    current_step: i32,
    total_steps: i32,
    source_public_snapshot_id: Option<i64>,
    source_final_snapshot_id: Option<i64>,
    plan_sha256: String,
    created_by_user_id: Option<i64>,
    started_at: Option<OffsetDateTime>,
    completed_at: Option<OffsetDateTime>,
    auto_play_enabled: bool,
    auto_play_interval_ms: i32,
    next_auto_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    version: i32,
    state_data: String,
}

impl RunRow {
    fn response(self) -> Result<ResolverRunResponse, AppError> {
        Ok(ResolverRunResponse {
            id: self.id,
            contest_id: self.contest_id,
            official: self.official,
            status: self.status,
            current_step: self.current_step,
            total_steps: self.total_steps,
            source_public_snapshot_id: self.source_public_snapshot_id.ok_or_else(|| {
                AppError::internal("load resolver run", "run has no public source snapshot")
            })?,
            source_final_snapshot_id: self.source_final_snapshot_id.ok_or_else(|| {
                AppError::internal("load resolver run", "run has no final source snapshot")
            })?,
            plan_sha256: self.plan_sha256,
            created_by_user_id: self
                .created_by_user_id
                .ok_or_else(|| AppError::internal("load resolver run", "run has no creator"))?,
            started_at: self.started_at,
            completed_at: self.completed_at,
            auto_play_enabled: self.auto_play_enabled,
            auto_play_interval_milliseconds: self.auto_play_interval_ms,
            next_auto_at: self.next_auto_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
            state: serde_json::from_str(&self.state_data)
                .map_err(|error| AppError::internal("decode resolver state", error))?,
        })
    }
}

pub struct ResolverService {
    database: PgPool,
}

impl ResolverService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn create(
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

    async fn get(&self, id: i64, actor: &AuthUser) -> Result<ResolverRunResponse, AppError> {
        require_operator(actor)?;
        load_run(&self.database, id).await
    }

    async fn list(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ResolverRunResponse>, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest_id).await?;
        sqlx::query_as::<_, RunRow>(safe_sql!(
            "{RUN_SELECT} WHERE run.contest_id = $1 ORDER BY run.official DESC, run.created_at DESC"
        ))
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list resolver runs", error))?
        .into_iter()
        .map(RunRow::response)
        .collect()
    }

    async fn sources(
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

    async fn public_state(&self, id: i64) -> Result<ResolverPublicStateResponse, AppError> {
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

    async fn events(
        &self,
        id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ResolverEventResponse>, AppError> {
        require_operator(actor)?;
        sqlx::query_as::<_, EventRow>(
            "SELECT event.id, event.event_type, event.payload, event.sequence, event.actor_user_id, event.created_at FROM resolver_events event JOIN resolver_runs run ON run.id = event.run_id JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL WHERE event.run_id = $1 ORDER BY event.sequence",
        )
        .bind(id).fetch_all(&self.database).await
        .map_err(|error| AppError::internal("list resolver events", error))?
        .into_iter().map(EventRow::response).collect()
    }

    async fn command(
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

    async fn configure_auto_play(
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

pub struct ResolverAutoRunner {
    database: PgPool,
}

impl ResolverAutoRunner {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!("Resolver auto-play runner started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.advance_due().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => warn!(%error, "Resolver auto-play advance failed"),
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() { break; }
                }
            }
        }
        info!("Resolver auto-play runner stopped");
    }

    async fn advance_due(&self) -> Result<bool, sqlx::Error> {
        let mut tx = self.database.begin().await?;
        let advanced = sqlx::query_as::<_, (i64, i64, bool, i32, i64)>(
            r#"
            WITH candidate AS (
                SELECT run.id FROM resolver_runs run
                JOIN contests contest ON contest.id = run.contest_id AND contest.deleted_at IS NULL
                WHERE run.auto_play_enabled AND run.status = 'RUNNING' AND run.next_auto_at <= now()
                ORDER BY run.next_auto_at, run.id FOR UPDATE OF run SKIP LOCKED LIMIT 1
            )
            UPDATE resolver_runs run SET current_step = run.current_step + 1,
                auto_play_enabled = run.current_step + 1 < run.total_steps,
                next_auto_at = CASE WHEN run.current_step + 1 < run.total_steps
                    THEN now() + run.auto_play_interval_ms * interval '1 millisecond' ELSE NULL END,
                updated_at = now(), version = version + 1
            FROM candidate WHERE run.id = candidate.id
            RETURNING run.id, run.contest_id, run.official, run.current_step,
                coalesce(run.created_by_user_id, 0)
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some((run_id, contest_id, official, step, actor)) = advanced else {
            tx.rollback().await?;
            return Ok(false);
        };
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
        .bind(run_id)
        .bind(step)
        .execute(&mut *tx)
        .await?;
        let sequence = sqlx::query_scalar::<_, i32>(
            "SELECT coalesce(max(sequence), -1) + 1 FROM resolver_events WHERE run_id = $1",
        )
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO resolver_events (run_id, event_type, payload, sequence, actor_user_id) VALUES ($1, 'AUTO_NEXT', $2, $3, nullif($4, 0))")
            .bind(run_id).bind(json!({"stepIndex": step}).to_string()).bind(sequence).bind(actor)
            .execute(&mut *tx).await?;
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'RESOLVER_STATE_CHANGED', $3, $4)")
            .bind(Uuid::new_v4()).bind(contest_id).bind(if official { "PUBLIC" } else { "STAFF" })
            .bind(json!({"resolverRunId": run_id, "action": "AUTO_NEXT", "stepIndex": step, "status": "RUNNING"}))
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}

struct SourceSnapshot {
    contest_id: i64,
    variant: String,
    frozen: bool,
    sha256: String,
    board: ScoreboardResponse,
}

async fn load_source_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<SourceSnapshot, AppError> {
    let row = sqlx::query_as::<_, (i64, String, bool, String, Option<String>)>(
        "SELECT contest_id, variant, frozen, payload_json, payload_sha256 FROM scoreboard_snapshots WHERE id = $1",
    ).bind(id).fetch_optional(&mut **tx).await
        .map_err(|error| AppError::internal("load Resolver source snapshot", error))?
        .ok_or_else(|| AppError::not_found("SCOREBOARD_SNAPSHOT_NOT_FOUND", "Source snapshot was not found"))?;
    Ok(SourceSnapshot {
        contest_id: row.0,
        variant: row.1,
        frozen: row.2,
        board: serde_json::from_str(&row.3)
            .map_err(|error| AppError::internal("decode Resolver source snapshot", error))?,
        sha256: row.4.ok_or_else(|| {
            AppError::internal("load Resolver source snapshot", "snapshot has no SHA-256")
        })?,
    })
}

fn build_states(
    mut current: ScoreboardResponse,
    final_board: ScoreboardResponse,
) -> Result<Vec<ResolverState>, AppError> {
    if current.contest_id != final_board.contest_id {
        return Err(AppError::validation("snapshots", "must describe the same contest"));
    }
    let final_rows =
        final_board.rows.iter().map(|row| (row.team_id, row)).collect::<HashMap<_, _>>();
    if current.rows.len() != final_rows.len()
        || current.problems.len() != final_board.problems.len()
        || current.problems.iter().any(|problem| {
            !final_board.problems.iter().any(|candidate| candidate.problem_id == problem.problem_id)
        })
    {
        return Err(AppError::validation("snapshots", "team and problem sets must match exactly"));
    }
    let mut pending = Vec::new();
    for row in &current.rows {
        let final_row = final_rows
            .get(&row.team_id)
            .ok_or_else(|| AppError::validation("snapshots", "team sets do not match"))?;
        for cell in &row.problems {
            let final_cell = final_row
                .problems
                .iter()
                .find(|candidate| candidate.problem_id == cell.problem_id)
                .ok_or_else(|| AppError::validation("snapshots", "problem sets do not match"))?;
            if cell != final_cell {
                pending.push((row.team_id, cell.problem_id));
            }
        }
    }
    let total_steps = i32::try_from(pending.len())
        .map_err(|error| AppError::internal("count resolver plan", error))?;
    let mut states = vec![ResolverState {
        step_index: 0,
        total_steps,
        board: current.clone(),
        last_reveal: None,
    }];
    while !pending.is_empty() {
        pending.sort_by_key(|(team_id, problem_id)| {
            let rank =
                current.rows.iter().find(|row| row.team_id == *team_id).map_or(0, |row| row.rank);
            (std::cmp::Reverse(rank), *team_id, *problem_id)
        });
        let (team_id, problem_id) = pending.remove(0);
        let row =
            current.rows.iter_mut().find(|row| row.team_id == team_id).ok_or_else(|| {
                AppError::internal("build resolver plan", "current team disappeared")
            })?;
        let final_row = final_rows[&team_id];
        let cell = row.problems.iter_mut().find(|cell| cell.problem_id == problem_id).ok_or_else(
            || AppError::internal("build resolver plan", "current problem disappeared"),
        )?;
        let final_cell =
            final_row.problems.iter().find(|cell| cell.problem_id == problem_id).ok_or_else(
                || AppError::internal("build resolver plan", "final problem disappeared"),
            )?;
        let before = cell.clone();
        *cell = final_cell.clone();
        let reveal = Reveal { team_id, problem_id, before, after: final_cell.clone() };
        recompute_board(&mut current);
        let step_index = i32::try_from(states.len())
            .map_err(|error| AppError::internal("convert resolver plan step", error))?;
        states.push(ResolverState {
            step_index,
            total_steps,
            board: current.clone(),
            last_reveal: Some(reveal),
        });
    }
    Ok(states)
}

fn recompute_board(board: &mut ScoreboardResponse) {
    for row in &mut board.rows {
        row.solved_count = i32::try_from(row.problems.iter().filter(|cell| cell.solved).count())
            .unwrap_or(i32::MAX);
        row.penalty_minutes =
            row.problems.iter().filter(|cell| cell.solved).map(|cell| cell.penalty_minutes).sum();
        row.last_solved_at = row.problems.iter().filter_map(|cell| cell.solved_at).max();
        for cell in &mut row.problems {
            cell.first_blood = false;
        }
    }
    board.rows.sort_by(compare_rows);
    let mut official_rank = 0_u32;
    for (index, row) in board.rows.iter_mut().enumerate() {
        row.rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        row.official_rank = if row.participation_type == "OFFICIAL" {
            official_rank = official_rank.saturating_add(1);
            Some(official_rank)
        } else {
            None
        };
    }
    for problem in &mut board.problems {
        let first = board
            .rows
            .iter()
            .filter(|row| row.participation_type != "PRACTICE")
            .filter_map(|row| {
                row.problems
                    .iter()
                    .find(|cell| cell.problem_id == problem.problem_id && cell.solved)
                    .and_then(|cell| cell.solved_at.map(|at| (at, row.team_id)))
            })
            .min();
        problem.first_blood_at = first.map(|value| value.0);
        problem.first_blood_team_id = first.map(|value| value.1);
        if let Some((_, team_id)) = first
            && let Some(cell) =
                board.rows.iter_mut().find(|row| row.team_id == team_id).and_then(|row| {
                    row.problems.iter_mut().find(|cell| cell.problem_id == problem.problem_id)
                })
        {
            cell.first_blood = true;
        }
    }
}

fn compare_rows(left: &ScoreboardRow, right: &ScoreboardRow) -> Ordering {
    right
        .solved_count
        .cmp(&left.solved_count)
        .then_with(|| left.penalty_minutes.cmp(&right.penalty_minutes))
        .then_with(|| match (left.last_solved_at, right.last_solved_at) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| left.team_id.cmp(&right.team_id))
}

fn encode_state(state: &ResolverState) -> Result<(String, String), AppError> {
    let encoded = serde_json::to_string(state)
        .map_err(|error| AppError::internal("encode resolver state", error))?;
    let sha = hex::encode(Sha256::digest(encoded.as_bytes()));
    Ok((encoded, sha))
}

const RUN_SELECT: &str = r#"SELECT run.id, run.contest_id, run.official, run.status,
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
    sqlx::query_as::<_, RunRow>(safe_sql!("{RUN_SELECT} WHERE run.id = $1"))
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

#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/resolver-runs", operation_id = "createResolverRun", tag = "resolver", params(("contest_id" = i64, Path)), request_body = CreateRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain source snapshot identifiers"))?;
    Ok(Json(state.resolver().create(contest_id, request, context.user(), peer.ip()).await?))
}

#[utoipa::path(get, path = "/api/admin/resolver-runs/{id}", operation_id = "getResolverRun", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = ResolverRunResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().get(id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/resolver-runs", operation_id = "listResolverRuns", tag = "resolver", params(("contest_id" = i64, Path)), responses((status = 200, body = [ResolverRunResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<ResolverRunResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().list(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/resolver-sources", operation_id = "getResolverSources", tag = "resolver", params(("contest_id" = i64, Path)), responses((status = 200, body = ResolverSourcesResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn sources(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<ResolverSourcesResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().sources(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/public/resolver-runs/{id}/state", operation_id = "getPublicResolverState", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = ResolverPublicStateResponse), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn public_state(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ResolverPublicStateResponse>, AppError> {
    Ok(Json(state.resolver().public_state(id).await?))
}

#[utoipa::path(get, path = "/api/admin/resolver-runs/{id}/events", operation_id = "listResolverEvents", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = [ResolverEventResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn events(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ResolverEventResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().events(id, context.user()).await?))
}

macro_rules! command_handler {
    ($name:ident, $action:literal, $path:literal, $operation:literal) => {
        #[utoipa::path(post, path = $path, operation_id = $operation, tag = "resolver", params(("id" = i64, Path)), request_body = CommandRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
        pub async fn $name(
            context: AuthContext,
            State(state): State<AppState>,
            ConnectInfo(peer): ConnectInfo<SocketAddr>,
            Path(id): Path<i64>,
            payload: Result<Json<CommandRequest>, JsonRejection>,
        ) -> Result<Json<ResolverRunResponse>, AppError> {
            context.require_password_ready()?;
            let Json(request) = payload
                .map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
            Ok(Json(
                state
                    .resolver()
                    .command(id, $action, request.expected_version, context.user(), peer.ip())
                    .await?,
            ))
        }
    };
}

command_handler!(start, "START", "/api/admin/resolver-runs/{id}/start", "startResolverRun");
command_handler!(next, "NEXT", "/api/admin/resolver-runs/{id}/next", "nextResolverStep");
command_handler!(
    previous,
    "PREVIOUS",
    "/api/admin/resolver-runs/{id}/previous",
    "previousResolverStep"
);
command_handler!(pause, "PAUSE", "/api/admin/resolver-runs/{id}/pause", "pauseResolverRun");
command_handler!(resume, "RESUME", "/api/admin/resolver-runs/{id}/resume", "resumeResolverRun");
command_handler!(
    complete,
    "COMPLETE",
    "/api/admin/resolver-runs/{id}/complete",
    "completeResolverRun"
);

#[utoipa::path(post, path = "/api/admin/resolver-runs/{id}/auto-play", operation_id = "configureResolverAutoPlay", tag = "resolver", params(("id" = i64, Path)), request_body = AutoPlayRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn auto_play(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<AutoPlayRequest>, JsonRejection>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload.map_err(|_| {
        AppError::validation(
            "request",
            "must contain expectedVersion, enabled, and intervalMilliseconds",
        )
    })?;
    Ok(Json(state.resolver().configure_auto_play(id, request, context.user(), peer.ip()).await?))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sha2::{Digest, Sha256};
    use sqlx::PgPool;

    use super::{
        AutoPlayRequest, CreateRequest, ResolverAutoRunner, ResolverService, build_states,
    };
    use crate::features::auth::model::{AuthUser, UserType};
    use crate::features::scoreboard::{
        ScoreboardCell, ScoreboardProblem, ScoreboardResponse, ScoreboardRow,
    };
    use time::OffsetDateTime;

    fn board(solved: bool) -> ScoreboardResponse {
        let solved_at = solved.then(OffsetDateTime::now_utc);
        ScoreboardResponse {
            contest_id: 1,
            variant: "PUBLIC".into(),
            frozen: true,
            scoring_mode: "ICPC".into(),
            score_aggregation: "BEST".into(),
            generated_at: OffsetDateTime::now_utc(),
            problems: vec![ScoreboardProblem {
                problem_id: 1,
                alias: "A".into(),
                display_order: 1,
                first_blood_team_id: None,
                first_blood_at: None,
            }],
            rows: vec![ScoreboardRow {
                rank: 1,
                official_rank: Some(1),
                team_id: 1,
                team_name: "Team".into(),
                school: None,
                participation_type: "OFFICIAL".into(),
                group_name: None,
                is_star: false,
                solved_count: i32::from(solved),
                penalty_minutes: if solved { 60 } else { 0 },
                total_score_milli: if solved { 100_000 } else { 0 },
                last_solved_at: solved_at,
                problems: vec![ScoreboardCell {
                    problem_id: 1,
                    wrong_attempts: 0,
                    solved,
                    solved_at,
                    penalty_minutes: if solved { 60 } else { 0 },
                    score_milli: if solved { 100_000 } else { 0 },
                    first_blood: solved,
                }],
            }],
        }
    }

    #[test]
    fn plan_is_deterministic_and_reaches_final_cell() {
        let states = build_states(board(false), board(true)).expect("build plan");
        assert_eq!(states.len(), 2);
        assert_eq!(states[1].step_index, 1);
        assert!(states[1].board.rows[0].problems[0].solved);
        assert_eq!(states[1].board.rows[0].solved_count, 1);
    }

    #[test]
    fn public_state_does_not_expose_internal_run_metadata() {
        let response = super::ResolverPublicStateResponse {
            id: 9,
            contest_id: 7,
            status: "RUNNING".to_owned(),
            current_step: 1,
            total_steps: 2,
            updated_at: OffsetDateTime::now_utc(),
            state: serde_json::json!({"stepIndex": 1}),
        };
        let value = serde_json::to_value(response).expect("serialize public Resolver state");
        assert!(value.get("state").is_some());
        assert!(value.get("createdByUserId").is_none());
        assert!(value.get("sourcePublicSnapshotId").is_none());
        assert!(value.get("planSha256").is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn official_run_is_immutable_reversible_and_restart_safe(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('resolver-op', 'hash', 'Resolver Operator', 'STAFF') RETURNING id")
            .fetch_one(&pool).await.expect("insert Resolver operator");
        let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at) VALUES ('Resolver Contest', 'ENDED', 'PUBLIC', now() - interval '3 hours', now() - interval '2 hours', now() - interval '1 hour') RETURNING id")
            .fetch_one(&pool).await.expect("insert Resolver contest");
        let public_board = board(false);
        let public_payload = serde_json::to_string(&public_board).expect("encode public board");
        let public_sha = hex::encode(Sha256::digest(public_payload.as_bytes()));
        let public_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'PUBLIC', 1, true, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(public_payload).bind(public_sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert public source snapshot");
        let mut final_board = board(true);
        final_board.contest_id = contest_id;
        final_board.variant = "ADMIN".to_owned();
        final_board.frozen = false;
        let final_payload = serde_json::to_string(&final_board).expect("encode final board");
        let final_sha = hex::encode(Sha256::digest(final_payload.as_bytes()));
        let final_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'ADMIN', 1, false, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(final_payload).bind(final_sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert final source snapshot");
        sqlx::query("UPDATE scoreboard_snapshots SET payload_json = replace(payload_json, '\"contestId\":1', $2) WHERE id = $1")
            .bind(public_id).bind(format!("\"contestId\":{contest_id}"))
            .execute(&pool).await.expect_err("source snapshots are immutable");
        // Build the public payload with the actual contest before inserting a replacement snapshot.
        let mut actual_public = board(false);
        actual_public.contest_id = contest_id;
        let payload = serde_json::to_string(&actual_public).expect("encode actual public board");
        let sha = hex::encode(Sha256::digest(payload.as_bytes()));
        let actual_public_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'PUBLIC', 2, true, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(payload).bind(sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert actual public source snapshot");
        let actor = AuthUser {
            id: user_id,
            username: "resolver-op".to_owned(),
            display_name: "Resolver Operator".to_owned(),
            user_type: UserType::Staff,
            permissions: vec!["RESOLVER_MANAGE".to_owned()],
            password_reset_required: false,
        };
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let service = ResolverService::new(pool.clone());
        let sources = service.sources(contest_id, &actor).await.expect("load Resolver sources");
        assert_eq!(
            (sources.public_snapshot.id, sources.final_snapshot.id),
            (actual_public_id, final_id)
        );
        let created = service
            .create(
                contest_id,
                CreateRequest {
                    public_snapshot_id: actual_public_id,
                    final_snapshot_id: final_id,
                    official: true,
                },
                &actor,
                ip,
            )
            .await
            .expect("create official Resolver run");
        assert_eq!(
            (created.status.as_str(), created.current_step, created.total_steps),
            ("READY", 0, 1)
        );
        assert_eq!(service.list(contest_id, &actor).await.expect("list runs").len(), 1);
        assert!(service.public_state(created.id).await.is_err());

        let started = service.command(created.id, "START", 0, &actor, ip).await.expect("start");
        assert_eq!(started.status, "RUNNING");
        assert!(service.public_state(created.id).await.is_ok());
        let advanced = service.command(created.id, "NEXT", 1, &actor, ip).await.expect("next");
        assert_eq!(advanced.state["board"]["rows"][0]["solvedCount"], 1);
        let backed =
            service.command(created.id, "PREVIOUS", 2, &actor, ip).await.expect("previous");
        assert_eq!(backed.current_step, 0);
        let advanced =
            service.command(created.id, "NEXT", 3, &actor, ip).await.expect("next again");
        let paused = service.command(created.id, "PAUSE", 4, &actor, ip).await.expect("pause");
        let resumed = service.command(created.id, "RESUME", 5, &actor, ip).await.expect("resume");
        let completed =
            service.command(created.id, "COMPLETE", 6, &actor, ip).await.expect("complete");
        assert_eq!(
            (
                advanced.current_step,
                paused.status.as_str(),
                resumed.status.as_str(),
                completed.status.as_str()
            ),
            (1, "PAUSED", "RUNNING", "COMPLETED")
        );
        assert!(completed.completed_at.is_some());

        let recovered = ResolverService::new(pool.clone())
            .get(created.id, &actor)
            .await
            .expect("recover after restart");
        assert_eq!((recovered.status.as_str(), recovered.current_step), ("COMPLETED", 1));
        assert_eq!(service.events(created.id, &actor).await.expect("events").len(), 8);
        let preview = service
            .create(
                contest_id,
                CreateRequest {
                    public_snapshot_id: actual_public_id,
                    final_snapshot_id: final_id,
                    official: false,
                },
                &actor,
                ip,
            )
            .await
            .expect("create preview Resolver run");
        let preview =
            service.command(preview.id, "START", 0, &actor, ip).await.expect("start preview");
        let preview = service
            .configure_auto_play(
                preview.id,
                AutoPlayRequest {
                    expected_version: preview.version,
                    enabled: true,
                    interval_milliseconds: 500,
                },
                &actor,
                ip,
            )
            .await
            .expect("enable auto-play");
        sqlx::query("UPDATE resolver_runs SET next_auto_at = now() WHERE id = $1")
            .bind(preview.id)
            .execute(&pool)
            .await
            .expect("make auto-play due");
        assert!(ResolverAutoRunner::new(pool.clone()).advance_due().await.expect("auto advance"));
        let auto_advanced = service.get(preview.id, &actor).await.expect("load auto advance");
        assert_eq!(auto_advanced.current_step, 1);
        assert!(!auto_advanced.auto_play_enabled);
        assert!(sqlx::query("UPDATE resolver_snapshots SET state_data = '{}' WHERE run_id = $1 AND step_index = 0")
            .bind(created.id).execute(&pool).await.is_err());
        assert!(sqlx::query("UPDATE resolver_runs SET source_final_snapshot_id = source_public_snapshot_id WHERE id = $1")
            .bind(created.id).execute(&pool).await.is_err());
    }
}
