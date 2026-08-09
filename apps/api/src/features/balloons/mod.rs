use std::net::{IpAddr, SocketAddr};

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{AuthContext, model::AuthUser},
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchQuery {
    pub limit: Option<i32>,
    pub zone: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DispatchPolicyResponse {
    contest_id: i64,
    strategy: String,
    max_batch: i32,
    cooldown_seconds: i32,
    zone_order: serde_json::Value,
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchPolicyRequest {
    pub strategy: String,
    pub max_batch: i32,
    pub cooldown_seconds: i32,
    pub zone_order: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRequest {
    expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelRequest {
    expected_version: i32,
    reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteRequest {
    expected_version: i32,
    note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BalloonTaskResponse {
    id: i64,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
    submission_id: i64,
    color: String,
    is_first_blood: bool,
    status: String,
    seat_no: Option<String>,
    team_name: String,
    problem_alias: String,
    note: Option<String>,
    claimed_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    claimed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    delivered_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    cancelled_at: Option<OffsetDateTime>,
    cancelled_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    version: i32,
    reopened_count: i32,
    priority: i32,
    delivery_zone: String,
    dispatch_attempts: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    last_dispatched_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BalloonStatsResponse {
    total: i64,
    pending: i64,
    claimed: i64,
    delivered: i64,
    cancelled: i64,
    first_blood: i64,
}

pub struct BalloonService {
    database: PgPool,
}

impl BalloonService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn list(
        &self,
        contest_id: i64,
        status: Option<String>,
        actor: &AuthUser,
    ) -> Result<Vec<BalloonTaskResponse>, AppError> {
        require_operator(actor)?;
        ensure_contest(&self.database, contest_id).await?;
        sqlx::query_as::<_, BalloonTaskResponse>(safe_sql!(
            "{SELECT_COLUMNS} WHERE task.contest_id = $1 AND ($2::text IS NULL OR task.status = $2) ORDER BY task.is_first_blood DESC, task.created_at, task.id LIMIT 2000"
        ))
        .bind(contest_id)
        .bind(status)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list balloon tasks", error))
    }

    async fn stats(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<BalloonStatsResponse, AppError> {
        require_operator(actor)?;
        ensure_contest(&self.database, contest_id).await?;
        sqlx::query_as(
            r#"
            SELECT count(*) AS total,
                count(*) FILTER (WHERE status = 'PENDING') AS pending,
                count(*) FILTER (WHERE status = 'CLAIMED') AS claimed,
                count(*) FILTER (WHERE status = 'DELIVERED') AS delivered,
                count(*) FILTER (WHERE status = 'CANCELLED') AS cancelled,
                count(*) FILTER (WHERE is_first_blood) AS first_blood
            FROM balloon_tasks WHERE contest_id = $1
            "#,
        )
        .bind(contest_id)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("load balloon statistics", error))
    }

    async fn transition(
        &self,
        id: i64,
        action: &'static str,
        expected_version: i32,
        reason: Option<String>,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<BalloonTaskResponse, AppError> {
        require_operator(actor)?;
        if expected_version < 0 {
            return Err(AppError::validation("expectedVersion", "must not be negative"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin balloon transition", error))?;
        let (contest_id, status, version) = sqlx::query_as::<_, (i64, String, i32)>(
            "SELECT task.contest_id, task.status, task.version FROM balloon_tasks task JOIN contests contest ON contest.id = task.contest_id AND contest.deleted_at IS NULL WHERE task.id = $1 FOR UPDATE OF task",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock balloon task", error))?
        .ok_or_else(task_not_found)?;
        if version != expected_version {
            return Err(AppError::conflict(
                "BALLOON_VERSION_STALE",
                "Balloon task changed; reload and retry",
            ));
        }
        match action {
            "CLAIM" if status == "PENDING" => {
                sqlx::query("UPDATE balloon_tasks SET status = 'CLAIMED', claimed_by = $2, claimed_at = now(), updated_at = now(), version = version + 1 WHERE id = $1")
                    .bind(id).bind(actor.id).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("claim balloon task", error))?;
            }
            // Any operator may deliver a claimed task. Delivery is terminal and
            // guarded by the optimistic version check, so a different operator
            // can finish a batch the claiming operator left behind. The task is
            // re-assigned to whoever actually delivered it for accurate records.
            "DELIVER" if status == "CLAIMED" => {
                sqlx::query("UPDATE balloon_tasks SET status = 'DELIVERED', claimed_by = $2, delivered_at = now(), updated_at = now(), version = version + 1 WHERE id = $1")
                    .bind(id).bind(actor.id).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("deliver balloon task", error))?;
            }
            "CANCEL" if matches!(status.as_str(), "PENDING" | "CLAIMED") => {
                let reason = validate_reason(reason)?;
                sqlx::query("UPDATE balloon_tasks SET status = 'CANCELLED', delivered_at = NULL, cancelled_at = now(), cancelled_reason = $2, updated_at = now(), version = version + 1 WHERE id = $1")
                    .bind(id).bind(reason).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("cancel balloon task", error))?;
            }
            "REOPEN" if status == "CANCELLED" => {
                sqlx::query("UPDATE balloon_tasks SET status = 'PENDING', claimed_by = NULL, claimed_at = NULL, delivered_at = NULL, cancelled_at = NULL, cancelled_reason = NULL, dispatch_attempts = 0, last_dispatched_at = NULL, reopened_count = reopened_count + 1, updated_at = now(), version = version + 1 WHERE id = $1")
                    .bind(id).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("reopen balloon task", error))?;
            }
            _ => {
                return Err(AppError::conflict(
                    "BALLOON_STATE_CHANGED",
                    "Balloon task cannot perform this transition",
                ));
            }
        }
        audit_tx(&mut tx, actor.id, action, id, ip).await?;
        event_tx(&mut tx, contest_id, id, action).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit balloon transition", error))?;
        load(&self.database, id).await
    }

    async fn note(
        &self,
        id: i64,
        request: NoteRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<BalloonTaskResponse, AppError> {
        require_operator(actor)?;
        if request.expected_version < 0 {
            return Err(AppError::validation("expectedVersion", "must not be negative"));
        }
        let note = validate_note(request.note)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin balloon note update", error))?;
        let contest_id = sqlx::query_scalar::<_, i64>(
            "UPDATE balloon_tasks task SET note = $2, updated_at = now(), version = version + 1 WHERE task.id = $1 AND task.version = $3 AND EXISTS (SELECT 1 FROM contests contest WHERE contest.id = task.contest_id AND contest.deleted_at IS NULL) RETURNING task.contest_id",
        )
        .bind(id)
        .bind(note)
        .bind(request.expected_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("update balloon note", error))?
        .ok_or_else(|| {
            AppError::conflict(
                "BALLOON_VERSION_STALE",
                "Balloon task changed or no longer exists; reload and retry",
            )
        })?;
        audit_tx(&mut tx, actor.id, "NOTE", id, ip).await?;
        event_tx(&mut tx, contest_id, id, "NOTE").await?;
        tx.commit().await.map_err(|error| AppError::internal("commit balloon note", error))?;
        load(&self.database, id).await
    }

    async fn dispatch_policy(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<DispatchPolicyResponse, AppError> {
        require_operator(actor)?;
        ensure_contest(&self.database, contest_id).await?;
        Ok(sqlx::query_as::<_, DispatchPolicyResponse>("SELECT contest_id,strategy,max_batch,cooldown_seconds,zone_order::jsonb AS zone_order,updated_at FROM balloon_dispatch_policies WHERE contest_id=$1")
            .bind(contest_id).fetch_optional(&self.database).await.map_err(|e|AppError::internal("load balloon dispatch policy",e))?
            .unwrap_or(DispatchPolicyResponse { contest_id, strategy: "PRIORITY".into(), max_batch: 10, cooldown_seconds: 0, zone_order: json!([]), updated_at: OffsetDateTime::now_utc() }))
    }

    async fn update_dispatch_policy(
        &self,
        contest_id: i64,
        request: DispatchPolicyRequest,
        actor: &AuthUser,
    ) -> Result<DispatchPolicyResponse, AppError> {
        if !actor.has_role("SUPER_ADMIN") && !actor.has_role("CONTEST_ADMIN") {
            return Err(AppError::forbidden(
                "BALLOON_POLICY_ADMIN_REQUIRED",
                "Contest administrator access is required",
            ));
        }
        if !matches!(request.strategy.as_str(), "FIFO" | "PRIORITY" | "ZONE")
            || !(1..=100).contains(&request.max_batch)
            || !(0..=3600).contains(&request.cooldown_seconds)
            || request.zone_order.len() > 100
            || request.zone_order.iter().any(|v| v.trim().is_empty() || v.len() > 64)
        {
            return Err(AppError::validation(
                "dispatchPolicy",
                "strategy, limits, or zones are invalid",
            ));
        }
        ensure_contest(&self.database, contest_id).await?;
        if !actor.has_role("SUPER_ADMIN") {
            let assigned = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id = $1 AND user_id = $2)",
            )
            .bind(contest_id)
            .bind(actor.id)
            .fetch_one(&self.database)
            .await
            .map_err(|error| AppError::internal("check balloon policy scope", error))?;
            if !assigned {
                return Err(AppError::not_found("BALLOON_TASK_NOT_FOUND", "Contest was not found"));
            }
        }
        let zones = serde_json::to_string(&request.zone_order)
            .map_err(|e| AppError::internal("encode balloon zones", e))?;
        sqlx::query("INSERT INTO balloon_dispatch_policies(contest_id,strategy,max_batch,cooldown_seconds,zone_order,updated_by_user_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(contest_id) DO UPDATE SET strategy=EXCLUDED.strategy,max_batch=EXCLUDED.max_batch,cooldown_seconds=EXCLUDED.cooldown_seconds,zone_order=EXCLUDED.zone_order,updated_by_user_id=EXCLUDED.updated_by_user_id,updated_at=now()")
            .bind(contest_id).bind(request.strategy).bind(request.max_batch).bind(request.cooldown_seconds).bind(zones).bind(actor.id).execute(&self.database).await.map_err(|e|AppError::internal("save balloon dispatch policy",e))?;
        self.dispatch_policy(contest_id, actor).await
    }

    async fn dispatch(
        &self,
        contest_id: i64,
        query: DispatchQuery,
        actor: &AuthUser,
    ) -> Result<Vec<BalloonTaskResponse>, AppError> {
        require_operator(actor)?;
        let policy = self.dispatch_policy(contest_id, actor).await?;
        let limit = query.limit.unwrap_or(policy.max_batch);
        if limit < 1 || limit > policy.max_batch {
            return Err(AppError::validation("limit", "must be within the configured batch limit"));
        }
        let zone = query.zone.as_deref().map(str::trim).filter(|v| !v.is_empty());
        if zone.is_some_and(|value| value.len() > 64) {
            return Err(AppError::validation("zone", "must not exceed 64 characters"));
        }
        let zones: Vec<String> =
            serde_json::from_value(policy.zone_order.clone()).unwrap_or_default();
        let sql = format!(
            "WITH candidates AS (SELECT id FROM balloon_tasks WHERE contest_id=$1 AND status='PENDING' AND ($2::text IS NULL OR delivery_zone=$2) AND (last_dispatched_at IS NULL OR last_dispatched_at<=now()-make_interval(secs=>$3)) ORDER BY CASE WHEN $4='ZONE' THEN coalesce(array_position($5::text[],delivery_zone),2147483647) ELSE 0 END, CASE WHEN $4='PRIORITY' THEN priority ELSE 0 END DESC, is_first_blood DESC, created_at,id LIMIT $6 FOR UPDATE SKIP LOCKED), claimed AS (UPDATE balloon_tasks SET status='CLAIMED',claimed_by=$7,claimed_at=now(),last_dispatched_at=now(),dispatch_attempts=dispatch_attempts+1,updated_at=now(),version=version+1 WHERE id IN(SELECT id FROM candidates) RETURNING id) {SELECT_COLUMNS} JOIN claimed ON claimed.id=task.id ORDER BY task.priority DESC,task.created_at,task.id"
        );
        sqlx::query_as::<_, BalloonTaskResponse>(sqlx::AssertSqlSafe(sql))
            .bind(contest_id)
            .bind(zone)
            .bind(policy.cooldown_seconds)
            .bind(policy.strategy)
            .bind(zones)
            .bind(limit)
            .bind(actor.id)
            .fetch_all(&self.database)
            .await
            .map_err(|e| AppError::internal("dispatch balloon tasks", e))
    }
}

const SELECT_COLUMNS: &str = r#"SELECT task.id, task.contest_id, task.team_id, task.problem_id,
 task.submission_id, task.color, task.is_first_blood, task.status, task.seat_no,
 coalesce(task.team_name, '') AS team_name, coalesce(task.problem_alias, '') AS problem_alias,
 task.note, task.claimed_by AS claimed_by_user_id, task.claimed_at, task.delivered_at,
 task.cancelled_at, task.cancelled_reason, task.created_at, task.updated_at,
 task.version, task.reopened_count, task.priority, task.delivery_zone, task.dispatch_attempts,
 task.last_dispatched_at FROM balloon_tasks task"#;

pub(crate) async fn generate_for_accepted(
    tx: &mut Transaction<'_, Postgres>,
    submission_id: i64,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
    accepted: bool,
) -> Result<Option<i64>, sqlx::Error> {
    if !accepted {
        return Ok(None);
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("balloon:{contest_id}:{problem_id}"))
        .execute(&mut **tx)
        .await?;
    let task_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO balloon_tasks
            (contest_id, team_id, problem_id, submission_id, color, status,
             seat_no, team_name, problem_alias)
        SELECT submission.contest_id, submission.team_id, submission.problem_id, submission.id,
               problem.color, 'PENDING', team.seat_no, team.name, problem.alias
        FROM submissions submission
        JOIN contests contest ON contest.id = submission.contest_id
            AND contest.deleted_at IS NULL
            AND contest.status IN ('RUNNING', 'PAUSED')
            AND contest.freeze_at IS NOT NULL AND now() < contest.freeze_at
        JOIN contest_teams roster ON roster.contest_id = submission.contest_id
            AND roster.team_id = submission.team_id
            AND roster.participation_type IN ('OFFICIAL', 'STAR')
        JOIN contest_problems problem ON problem.contest_id = submission.contest_id
            AND problem.problem_id = submission.problem_id
            AND problem.color IS NOT NULL AND btrim(problem.color) <> ''
        JOIN teams team ON team.id = submission.team_id
        WHERE submission.id = $1 AND submission.contest_id = $2
            AND submission.team_id = $3 AND submission.problem_id = $4
        ON CONFLICT (contest_id, team_id, problem_id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(submission_id)
    .bind(contest_id)
    .bind(team_id)
    .bind(problem_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(task_id) = task_id else {
        return Ok(None);
    };
    sqlx::query("UPDATE balloon_tasks SET is_first_blood = false WHERE contest_id = $1 AND problem_id = $2 AND is_first_blood")
        .bind(contest_id).bind(problem_id).execute(&mut **tx).await?;
    sqlx::query(
        r#"
        UPDATE balloon_tasks task SET is_first_blood = true
        WHERE task.id = (
            SELECT candidate.id FROM balloon_tasks candidate
            JOIN submissions submission ON submission.id = candidate.submission_id
            WHERE candidate.contest_id = $1 AND candidate.problem_id = $2
              AND candidate.status <> 'CANCELLED'
            ORDER BY submission.submitted_at, candidate.team_id, candidate.submission_id
            LIMIT 1
        )
        "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'BALLOON_TASK_CREATED', 'STAFF', $3)")
        .bind(Uuid::new_v4()).bind(contest_id)
        .bind(json!({"balloonTaskId": task_id, "teamId": team_id, "problemId": problem_id}))
        .execute(&mut **tx).await?;
    Ok(Some(task_id))
}

async fn load(database: &PgPool, id: i64) -> Result<BalloonTaskResponse, AppError> {
    sqlx::query_as::<_, BalloonTaskResponse>(safe_sql!("{SELECT_COLUMNS} WHERE task.id = $1"))
        .bind(id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("load balloon task", error))?
        .ok_or_else(task_not_found)
}

async fn ensure_contest(database: &PgPool, contest_id: i64) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check balloon contest", error))?;
    if exists { Ok(()) } else { Err(task_not_found()) }
}

fn require_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") || actor.has_role("BALLOON_STAFF") {
        Ok(())
    } else {
        Err(AppError::forbidden("BALLOON_STAFF_REQUIRED", "Balloon staff role is required"))
    }
}

fn validate_status(status: Option<String>) -> Result<Option<String>, AppError> {
    status
        .map(|status| {
            let status = status.trim().to_ascii_uppercase();
            if matches!(status.as_str(), "PENDING" | "CLAIMED" | "DELIVERED" | "CANCELLED") {
                Ok(status)
            } else {
                Err(AppError::validation("status", "contains an unsupported balloon status"))
            }
        })
        .transpose()
}

fn validate_reason(reason: Option<String>) -> Result<String, AppError> {
    let reason = reason.unwrap_or_default().replace(['\r', '\n'], " ").trim().to_owned();
    if reason.is_empty() || reason.chars().count() > 255 {
        Err(AppError::validation("reason", "must contain 1 to 255 characters"))
    } else {
        Ok(reason)
    }
}

fn validate_note(note: Option<String>) -> Result<Option<String>, AppError> {
    note.map(|note| {
        let note = note.replace('\r', "").trim().to_owned();
        if note.chars().count() > 2000 {
            Err(AppError::validation("note", "must contain at most 2000 characters"))
        } else if note.is_empty() {
            Ok(None)
        } else {
            Ok(Some(note))
        }
    })
    .transpose()
    .map(Option::flatten)
}

async fn audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'BALLOON_TASK', $3, $4, 'success')")
        .bind(actor).bind(format!("BALLOON_{action}" )).bind(id.to_string()).bind(ip.to_string())
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("record balloon audit", error))
}

async fn event_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    id: i64,
    action: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'BALLOON_TASK_UPDATED', 'STAFF', $3)")
        .bind(Uuid::new_v4()).bind(contest_id)
        .bind(json!({"balloonTaskId": id, "action": action}))
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("enqueue balloon event", error))
}

fn task_not_found() -> AppError {
    AppError::not_found("BALLOON_TASK_NOT_FOUND", "Balloon task was not found")
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons", operation_id = "listBalloonTasks", tag = "balloons", params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [BalloonTaskResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Vec<BalloonTaskResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) = query.map_err(|_| AppError::validation("query", "invalid filters"))?;
    Ok(Json(
        state.balloons().list(contest_id, validate_status(query.status)?, context.user()).await?,
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons/stats", operation_id = "getBalloonStats", tag = "balloons", params(("contest_id" = i64, Path)), responses((status = 200, body = BalloonStatsResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn stats(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<BalloonStatsResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.balloons().stats(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons/dispatch-policy", operation_id = "getBalloonDispatchPolicy", tag = "balloons", params(("contest_id" = i64, Path)), responses((status = 200, body = DispatchPolicyResponse)), security(("session_cookie" = [])))]
pub async fn dispatch_policy(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<DispatchPolicyResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.balloons().dispatch_policy(contest_id, context.user()).await?))
}

#[utoipa::path(put, path = "/api/contests/{contest_id}/balloons/dispatch-policy", operation_id = "updateBalloonDispatchPolicy", tag = "balloons", params(("contest_id" = i64, Path)), request_body = DispatchPolicyRequest, responses((status = 200, body = DispatchPolicyResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_dispatch_policy(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<DispatchPolicyRequest>, JsonRejection>,
) -> Result<Json<DispatchPolicyResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid dispatch policy"))?;
    Ok(Json(state.balloons().update_dispatch_policy(contest_id, request, context.user()).await?))
}

#[utoipa::path(post, path = "/api/contests/{contest_id}/balloons/dispatch", operation_id = "dispatchBalloonTasks", tag = "balloons", params(("contest_id" = i64, Path), ("limit" = Option<i32>, Query), ("zone" = Option<String>, Query)), responses((status = 200, body = [BalloonTaskResponse])), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn dispatch(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<DispatchQuery>, QueryRejection>,
) -> Result<Json<Vec<BalloonTaskResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "invalid dispatch request"))?;
    Ok(Json(state.balloons().dispatch(contest_id, query, context.user()).await?))
}

async fn version_payload(
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<i32, AppError> {
    payload
        .map(|Json(request)| request.expected_version)
        .map_err(|_| AppError::validation("request", "must contain expectedVersion"))
}

#[utoipa::path(post, path = "/api/balloons/{id}/claim", operation_id = "claimBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn claim(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "CLAIM",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/deliver", operation_id = "deliverBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn deliver(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "DELIVER",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/cancel", operation_id = "cancelBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = CancelRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn cancel(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<CancelRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain expectedVersion and reason"))?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "CANCEL",
                request.expected_version,
                Some(request.reason),
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/reopen", operation_id = "reopenBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reopen(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "REOPEN",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(patch, path = "/api/balloons/{id}/note", operation_id = "updateBalloonNote", tag = "balloons", params(("id" = i64, Path)), request_body = NoteRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn note(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<NoteRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
    Ok(Json(state.balloons().note(id, request, context.user(), peer.ip()).await?))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use super::{BalloonService, validate_note, validate_reason, validate_status};
    use crate::features::auth::model::{AuthUser, UserType};

    #[test]
    fn balloon_input_domains_are_closed() {
        assert_eq!(
            validate_status(Some(" pending ".to_owned())).expect("status"),
            Some("PENDING".to_owned())
        );
        assert!(validate_status(Some("lost".to_owned())).is_err());
        assert!(validate_reason(Some("\n".to_owned())).is_err());
        assert_eq!(
            validate_note(Some("  note  ".to_owned())).expect("note"),
            Some("note".to_owned())
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn balloon_workbench_enforces_claim_ownership_and_recovery(pool: PgPool) {
        let first_user = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('balloon-one', 'hash', 'Balloon One', 'BALLOON_STAFF') RETURNING id")
            .fetch_one(&pool).await.expect("insert first balloon operator");
        let second_user = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('balloon-two', 'hash', 'Balloon Two', 'BALLOON_STAFF') RETURNING id")
            .fetch_one(&pool).await.expect("insert second balloon operator");
        let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at) VALUES ('Balloon Workbench', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour', now() + interval '2 hours') RETURNING id")
            .fetch_one(&pool).await.expect("insert balloon contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('balloon-a', 'Balloon A') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert balloon problem");
        let mut task_ids = Vec::new();
        for index in 1..=2 {
            let team_id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO teams (name, seat_no) VALUES ($1, $2) RETURNING id",
            )
            .bind(format!("Balloon Team {index}"))
            .bind(format!("A0{index}"))
            .fetch_one(&pool)
            .await
            .expect("insert balloon team");
            let submission_id = sqlx::query_scalar::<_, i64>("INSERT INTO submissions (contest_id, problem_id, team_id, language, source_object_key, source_size_bytes, source_sha256, status) VALUES ($1, $2, $3, 'cpp', $4, 10, $5, 'ACCEPTED') RETURNING id")
                .bind(contest_id).bind(problem_id).bind(team_id)
                .bind(format!("sources/balloon-{index}.cpp")).bind(format!("{index}").repeat(64))
                .fetch_one(&pool).await.expect("insert balloon submission");
            let task_id = sqlx::query_scalar::<_, i64>("INSERT INTO balloon_tasks (contest_id, team_id, problem_id, submission_id, color, status, seat_no, team_name, problem_alias) VALUES ($1, $2, $3, $4, '#ff0000', 'PENDING', $5, $6, 'A') RETURNING id")
                .bind(contest_id).bind(team_id).bind(problem_id).bind(submission_id)
                .bind(format!("A0{index}")).bind(format!("Balloon Team {index}"))
                .fetch_one(&pool).await.expect("insert balloon task");
            task_ids.push(task_id);
        }
        let actor = |id, username: &str| AuthUser {
            id,
            username: username.to_owned(),
            display_name: username.to_owned(),
            user_type: UserType::BalloonStaff,
            roles: vec!["BALLOON_STAFF".to_owned()],
            password_reset_required: false,
        };
        let first = actor(first_user, "balloon-one");
        let second = actor(second_user, "balloon-two");
        let service = BalloonService::new(pool.clone());
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        let claimed = service
            .transition(task_ids[0], "CLAIM", 0, None, &first, ip)
            .await
            .expect("claim balloon");
        assert_eq!(claimed.status, "CLAIMED");
        let noted = service
            .note(
                task_ids[0],
                super::NoteRequest { expected_version: 1, note: Some("gate west".to_owned()) },
                &first,
                ip,
            )
            .await
            .expect("note balloon");
        // Another operator can take over and deliver a claimed task; the task
        // is re-assigned to whoever actually delivered it.
        let delivered = service
            .transition(task_ids[0], "DELIVER", noted.version, None, &second, ip)
            .await
            .expect("another operator delivers a claimed balloon");
        assert_eq!(delivered.status, "DELIVERED");
        assert_eq!(delivered.claimed_by_user_id, Some(second.id));

        let cancelled = service
            .transition(task_ids[1], "CANCEL", 0, Some("team absent".to_owned()), &second, ip)
            .await
            .expect("cancel balloon");
        let reopened = service
            .transition(task_ids[1], "REOPEN", cancelled.version, None, &second, ip)
            .await
            .expect("reopen balloon");
        assert_eq!(reopened.status, "PENDING");
        assert_eq!(reopened.reopened_count, 1);
        let stats = service.stats(contest_id, &first).await.expect("balloon stats");
        assert_eq!((stats.total, stats.pending, stats.delivered), (2, 1, 1));
        assert_eq!(service.list(contest_id, None, &first).await.expect("list balloons").len(), 2);
        let (audits, events) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT (SELECT count(*) FROM audit_logs WHERE target_type = 'BALLOON_TASK'), (SELECT count(*) FROM realtime_outbox WHERE event_type = 'BALLOON_TASK_UPDATED')",
        )
        .fetch_one(&pool)
        .await
        .expect("count balloon side effects");
        assert_eq!((audits, events), (5, 5));
    }
}
