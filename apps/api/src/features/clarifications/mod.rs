use std::net::{IpAddr, SocketAddr};

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::{
        announcements::{
            AnnouncementResponse, audit_tx as announcement_audit, ensure_open_tx,
            load as load_announcement, public_event_tx, validate_text,
        },
        auth::{
            AuthContext,
            model::{AuthUser, UserType},
        },
    },
    state::AppState,
};

const RATE_LIMIT_MINUTES: i64 = 5;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AskRequest {
    scope: String,
    problem_id: Option<i64>,
    question: String,
}

struct ValidatedAsk {
    scope: &'static str,
    problem_id: Option<i64>,
    question: String,
}

impl AskRequest {
    fn validate(mut self) -> Result<ValidatedAsk, AppError> {
        let scope = match self.scope.trim().to_ascii_uppercase().as_str() {
            "GENERAL" if self.problem_id.is_none() => "GENERAL",
            "PROBLEM" if self.problem_id.is_some_and(|id| id > 0) => "PROBLEM",
            "GENERAL" | "PROBLEM" => {
                return Err(AppError::validation(
                    "problemId",
                    "must be absent for GENERAL and positive for PROBLEM",
                ));
            }
            _ => return Err(AppError::validation("scope", "must be GENERAL or PROBLEM")),
        };
        self.question = self.question.trim().to_owned();
        if self.question.is_empty() || self.question.chars().count() > 4000 {
            return Err(AppError::validation("question", "must contain 1 to 4000 characters"));
        }
        Ok(ValidatedAsk { scope, problem_id: self.problem_id, question: self.question })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplyRequest {
    reply: String,
    visibility: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConvertRequest {
    title: Option<String>,
    body: Option<String>,
}

struct ValidatedReply {
    reply: String,
    visibility: &'static str,
}

impl ReplyRequest {
    fn validate(mut self) -> Result<ValidatedReply, AppError> {
        self.reply = self.reply.trim().to_owned();
        if self.reply.is_empty() || self.reply.chars().count() > 8000 {
            return Err(AppError::validation("reply", "must contain 1 to 8000 characters"));
        }
        let visibility = match self.visibility.trim().to_ascii_uppercase().as_str() {
            "PRIVATE" => "PRIVATE",
            "PUBLIC" => "PUBLIC",
            _ => return Err(AppError::validation("visibility", "must be PRIVATE or PUBLIC")),
        };
        Ok(ValidatedReply { reply: self.reply, visibility })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAllQuery {
    status: Option<String>,
}

impl ListAllQuery {
    fn validate(self) -> Result<Option<String>, AppError> {
        self.status
            .map(|status| match status.trim().to_ascii_lowercase().as_str() {
                "pending" => Ok("PENDING".into()),
                "answered" => Ok("ANSWERED".into()),
                "closed" => Ok("CLOSED".into()),
                _ => Err(AppError::validation("status", "must be pending, answered, or closed")),
            })
            .transpose()
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationResponse {
    id: i64,
    contest_id: i64,
    team_id: i64,
    team_name: Option<String>,
    scope: String,
    problem_id: Option<i64>,
    problem_alias: Option<String>,
    question: String,
    status: String,
    reply: Option<String>,
    reply_visibility: Option<String>,
    asked_by_user_id: i64,
    replied_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    replied_at: Option<OffsetDateTime>,
    converted_announcement_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    version: i32,
}

pub struct ClarificationService {
    database: PgPool,
}

impl ClarificationService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn ask(
        &self,
        contest_id: i64,
        command: ValidatedAsk,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ClarificationResponse, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can ask a clarification",
            ));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification ask", error))?;
        let (team_id, team_name, contest_status) = sqlx::query_as::<_, (i64, String, String)>(
            r#"
            SELECT team.id, team.name, contest.status FROM team_accounts account
            JOIN teams team ON team.id = account.team_id AND team.deleted_at IS NULL
            JOIN contest_teams roster ON roster.team_id = team.id AND roster.contest_id = $2
            JOIN contests contest ON contest.id = roster.contest_id AND contest.deleted_at IS NULL
            WHERE account.user_id = $1
        "#,
        )
        .bind(actor.id)
        .bind(contest_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("resolve clarification team", error))?
        .ok_or_else(clarification_not_found)?;
        if !matches!(contest_status.as_str(), "RUNNING" | "PAUSED") {
            return Err(AppError::conflict(
                "CLARIFICATION_CONTEST_NOT_OPEN",
                "Clarifications are only open while a contest is running or paused",
            ));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("clarification:{contest_id}:{team_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("lock clarification rate limit", error))?;
        let recent = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM clarifications WHERE contest_id = $1 AND team_id = $2 AND created_at > now() - make_interval(mins => $3::integer))",
        ).bind(contest_id).bind(team_id).bind(RATE_LIMIT_MINUTES).fetch_one(&mut *tx).await
            .map_err(|error| AppError::internal("check clarification rate limit", error))?;
        if recent {
            return Err(AppError::too_many_requests(
                "CLARIFICATION_RATE_LIMITED",
                "A team may ask one clarification every five minutes",
            ));
        }
        let problem_alias = if let Some(problem_id) = command.problem_id {
            Some(
                sqlx::query_scalar::<_, String>(
                    "SELECT alias FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
                )
                .bind(contest_id)
                .bind(problem_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| AppError::internal("resolve clarification problem", error))?
                .ok_or_else(|| {
                    AppError::not_found("PROBLEM_NOT_FOUND", "Contest problem was not found")
                })?,
            )
        } else {
            None
        };
        let id = sqlx::query_scalar::<_, i64>(r#"
            INSERT INTO clarifications
                (contest_id, team_id, team_name, scope, problem_id, problem_alias, question, status, asked_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8) RETURNING id
        "#).bind(contest_id).bind(team_id).bind(team_name).bind(command.scope)
            .bind(command.problem_id).bind(problem_alias).bind(command.question).bind(actor.id)
            .fetch_one(&mut *tx).await
            .map_err(|error| AppError::internal("insert clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_ASKED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "ASKED").await?;
        tx.commit().await.map_err(|error| AppError::internal("commit clarification ask", error))?;
        load(&self.database, id).await
    }

    async fn list_mine(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ClarificationResponse>, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can view team clarifications",
            ));
        }
        sqlx::query_as::<_, ClarificationResponse>(&format!(
            "{SELECT_COLUMNS} JOIN team_accounts account ON account.team_id = clarification.team_id JOIN contest_teams roster ON roster.team_id = clarification.team_id AND roster.contest_id = clarification.contest_id WHERE clarification.contest_id = $1 AND account.user_id = $2 ORDER BY clarification.created_at DESC LIMIT 1000"
        )).bind(contest_id).bind(actor.id).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list team clarifications", error))
    }

    async fn list_all(
        &self,
        contest_id: i64,
        status: Option<String>,
        actor: &AuthUser,
    ) -> Result<Vec<ClarificationResponse>, AppError> {
        require_staff_access_pool(&self.database, contest_id, actor).await?;
        sqlx::query_as::<_, ClarificationResponse>(&format!(
            "{SELECT_COLUMNS} WHERE clarification.contest_id = $1 AND ($2::text IS NULL OR clarification.status = $2) ORDER BY clarification.created_at DESC LIMIT 1000"
        )).bind(contest_id).bind(status.as_deref()).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list contest clarifications", error))
    }

    async fn get(&self, id: i64, actor: &AuthUser) -> Result<ClarificationResponse, AppError> {
        let row = load(&self.database, id).await?;
        require_staff_access_pool(&self.database, row.contest_id, actor).await?;
        Ok(row)
    }

    async fn reply(
        &self,
        id: i64,
        command: ValidatedReply,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ClarificationResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification reply", error))?;
        let (contest_id, team_id, status) = lock_context(&mut tx, id).await?;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if status == "CLOSED" {
            return Err(AppError::conflict(
                "CLARIFICATION_CLOSED",
                "Closed clarification cannot be replied to",
            ));
        }
        sqlx::query("UPDATE clarifications SET reply = $2, reply_visibility = $3, replied_by = $4, replied_at = now(), status = 'ANSWERED', updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(command.reply).bind(command.visibility).bind(actor.id)
            .execute(&mut *tx).await.map_err(|error| AppError::internal("reply to clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_REPLIED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "REPLIED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification reply", error))?;
        load(&self.database, id).await
    }

    async fn close(&self, id: i64, actor: &AuthUser, request_ip: IpAddr) -> Result<(), AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification close", error))?;
        let (contest_id, team_id, status) = lock_context(&mut tx, id).await?;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if status == "CLOSED" {
            return Err(AppError::conflict(
                "CLARIFICATION_CLOSED",
                "Clarification is already closed",
            ));
        }
        sqlx::query("UPDATE clarifications SET status = 'CLOSED', closed_by = $2, closed_at = now(), updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(actor.id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("close clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_CLOSED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "CLOSED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification close", error))?;
        Ok(())
    }

    async fn convert(
        &self,
        id: i64,
        request: ConvertRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification conversion", error))?;
        let row = sqlx::query_as::<
            _,
            (i64, i64, String, String, Option<String>, Option<String>, Option<i64>),
        >(
            r#"
            SELECT contest_id, team_id, status, question, reply, reply_visibility,
                   converted_announcement_id
            FROM clarifications WHERE id = $1 FOR UPDATE
        "#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock clarification conversion", error))?
        .ok_or_else(clarification_not_found)?;
        let (contest_id, team_id, status, question, reply, visibility, converted_id) = row;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if converted_id.is_some() {
            return Err(AppError::conflict(
                "CLARIFICATION_ALREADY_CONVERTED",
                "Clarification has already been converted into an announcement",
            ));
        }
        if status != "ANSWERED" || visibility.as_deref() != Some("PUBLIC") {
            return Err(AppError::conflict(
                "CLARIFICATION_REPLY_NOT_PUBLIC",
                "Only an answered clarification with a PUBLIC reply can be converted",
            ));
        }
        ensure_open_tx(&mut tx, contest_id).await?;
        let default_title = question.chars().take(80).collect::<String>();
        let title = request.title.filter(|value| !value.trim().is_empty()).unwrap_or(default_title);
        let body =
            request.body.filter(|value| !value.trim().is_empty()).or(reply).ok_or_else(|| {
                AppError::conflict("CLARIFICATION_NOT_ANSWERED", "Clarification has no reply")
            })?;
        let (title, body) = validate_text(title, body)?;
        let announcement_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO announcements
                (contest_id, title, body, pinned, status, created_by,
                 source_clarification_id, published_at)
            VALUES ($1, $2, $3, false, 'PUBLISHED', $4, $5, now()) RETURNING id
        "#,
        )
        .bind(contest_id)
        .bind(title)
        .bind(body)
        .bind(actor.id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("insert clarification announcement", error))?;
        sqlx::query("UPDATE clarifications SET converted_announcement_id = $2, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(announcement_id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("link clarification announcement", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_CONVERTED", id, request_ip).await?;
        announcement_audit(
            &mut tx,
            actor.id,
            "ANNOUNCEMENT_PUBLISHED",
            announcement_id,
            request_ip,
        )
        .await?;
        realtime(&mut tx, contest_id, team_id, id, "CONVERTED").await?;
        public_event_tx(&mut tx, contest_id, announcement_id, "PUBLISHED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification conversion", error))?;
        load_announcement(&self.database, announcement_id).await
    }
}

const SELECT_COLUMNS: &str = r#"
    SELECT clarification.id, clarification.contest_id, clarification.team_id,
           clarification.team_name, clarification.scope, clarification.problem_id,
           clarification.problem_alias, clarification.question, clarification.status,
           clarification.reply, clarification.reply_visibility,
           clarification.asked_by AS asked_by_user_id,
           clarification.replied_by AS replied_by_user_id, clarification.replied_at,
           clarification.converted_announcement_id, clarification.created_at,
           clarification.updated_at, clarification.version FROM clarifications clarification
"#;

async fn load(database: &PgPool, id: i64) -> Result<ClarificationResponse, AppError> {
    if id <= 0 {
        return Err(clarification_not_found());
    }
    sqlx::query_as::<_, ClarificationResponse>(&format!(
        "{SELECT_COLUMNS} WHERE clarification.id = $1"
    ))
    .bind(id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load clarification", error))?
    .ok_or_else(clarification_not_found)
}

async fn lock_context(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<(i64, i64, String), AppError> {
    sqlx::query_as(
        "SELECT contest_id, team_id, status FROM clarifications WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| AppError::internal("lock clarification", error))?
    .ok_or_else(clarification_not_found)
}

async fn require_staff_access_pool(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let mut tx = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin clarification access check", error))?;
    require_staff_access(&mut tx, contest_id, actor).await?;
    tx.commit()
        .await
        .map_err(|error| AppError::internal("commit clarification access check", error))
}

async fn require_staff_access(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    // Judge workers are global staff operators; unlike contest administrators
    // they are intentionally not present in contest_admin_assignments.
    if actor.has_role("JUDGE") {
        return Ok(());
    }
    if !actor.has_role("CONTEST_ADMIN") {
        return Err(clarification_not_found());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id = $1 AND user_id = $2)",
    ).bind(contest_id).bind(actor.id).fetch_one(&mut **tx).await
        .map_err(|error| AppError::internal("check clarification staff scope", error))?;
    if assigned { Ok(()) } else { Err(clarification_not_found()) }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    action: &str,
    id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'CLARIFICATION', $3, $4, 'success')")
        .bind(actor_id).bind(action).bind(id.to_string()).bind(request_ip.to_string())
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("record clarification audit", error))
}

async fn realtime(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
    id: i64,
    action: &str,
) -> Result<(), AppError> {
    for (scope, recipient) in [("STAFF", None), ("TEAM", Some(team_id))] {
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, team_id, payload_json) VALUES ($1, $2, 'CLARIFICATION_UPDATED', $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(contest_id).bind(scope).bind(recipient)
            .bind(json!({"clarificationId": id, "action": action}))
            .execute(&mut **tx).await
            .map_err(|error| AppError::internal("enqueue clarification event", error))?;
    }
    Ok(())
}

fn clarification_not_found() -> AppError {
    AppError::not_found("CLARIFICATION_NOT_FOUND", "Clarification was not found")
}

#[utoipa::path(post, path = "/api/contests/{contest_id}/clarifications", operation_id = "askClarification", tag = "clarifications", params(("contest_id" = i64, Path)), request_body = AskRequest, responses((status = 201, body = ClarificationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 429, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn ask(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<AskRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ClarificationResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid clarification"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .clarifications()
                .ask(contest_id, request.validate()?, context.user(), peer.ip())
                .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/clarifications/mine", operation_id = "listOwnClarifications", tag = "clarifications", params(("contest_id" = i64, Path)), responses((status = 200, body = [ClarificationResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_mine(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<ClarificationResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.clarifications().list_mine(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/clarifications/all", operation_id = "listAllClarifications", tag = "clarifications", params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [ClarificationResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_all(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListAllQuery>, QueryRejection>,
) -> Result<Json<Vec<ClarificationResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "contains an invalid clarification status"))?;
    Ok(Json(state.clarifications().list_all(contest_id, query.validate()?, context.user()).await?))
}

#[utoipa::path(get, path = "/api/clarifications/{id}", operation_id = "getClarification", tag = "clarifications", params(("id" = i64, Path)), responses((status = 200, body = ClarificationResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ClarificationResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.clarifications().get(id, context.user()).await?))
}

#[utoipa::path(post, path = "/api/clarifications/{id}/reply", operation_id = "replyClarification", tag = "clarifications", params(("id" = i64, Path)), request_body = ReplyRequest, responses((status = 200, body = ClarificationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reply(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<ReplyRequest>, JsonRejection>,
) -> Result<Json<ClarificationResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid clarification reply"))?;
    Ok(Json(
        state.clarifications().reply(id, request.validate()?, context.user(), peer.ip()).await?,
    ))
}

#[utoipa::path(post, path = "/api/clarifications/{id}/close", operation_id = "closeClarification", tag = "clarifications", params(("id" = i64, Path)), responses((status = 204, description = "Clarification closed"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn close(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    context.require_password_ready()?;
    state.clarifications().close(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/clarifications/{id}/convert", operation_id = "convertClarification", tag = "clarifications", params(("id" = i64, Path)), request_body = ConvertRequest, responses((status = 200, body = crate::features::announcements::AnnouncementResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn convert(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<ConvertRequest>, JsonRejection>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid conversion request"))?;
    Ok(Json(state.clarifications().convert(id, request, context.user(), peer.ip()).await?))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use super::{AskRequest, ClarificationService, ConvertRequest, ReplyRequest};
    use crate::features::auth::model::{AuthUser, UserType};

    #[test]
    fn scope_problem_shape_and_reply_visibility_are_closed() {
        assert!(
            AskRequest { scope: "GENERAL".into(), problem_id: None, question: "Question".into() }
                .validate()
                .is_ok()
        );
        assert!(
            AskRequest {
                scope: "GENERAL".into(),
                problem_id: Some(1),
                question: "Question".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            AskRequest { scope: "PROBLEM".into(), problem_id: None, question: "Question".into() }
                .validate()
                .is_err()
        );
        assert!(
            ReplyRequest { reply: "Answer".into(), visibility: "PUBLIC".into() }.validate().is_ok()
        );
        assert!(
            ReplyRequest { reply: "Answer".into(), visibility: "GLOBAL".into() }
                .validate()
                .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn private_workflow_is_rate_limited_scoped_and_transactional(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-root', 'test-hash', 'Clar Root', 'SUPER_ADMIN') RETURNING id",
        )
        .fetch_one(&pool).await.expect("insert clarification administrator");
        let team_user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-team', 'test-hash', 'Clar Team', 'TEAM') RETURNING id",
        )
        .fetch_one(&pool).await.expect("insert clarification team account");
        let other_user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-other', 'test-hash', 'Other Team', 'TEAM') RETURNING id",
        )
        .fetch_one(&pool).await.expect("insert other team account");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Clar Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert clarification team");
        let other_team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Other Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert other team");
        for (user_id, linked_team_id) in [(team_user_id, team_id), (other_user_id, other_team_id)] {
            sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(linked_team_id)
                .execute(&pool)
                .await
                .expect("link clarification team account");
        }
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, end_at)
            VALUES ('Clarification Contest', 'RUNNING', 'PRIVATE',
                    now() - interval '1 hour', now() + interval '1 hour') RETURNING id
        "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert clarification contest");
        for linked_team_id in [team_id, other_team_id] {
            sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
                .bind(contest_id).bind(linked_team_id).execute(&pool).await
                .expect("roster clarification team");
        }
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('clar-a', 'Clar A') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert clarification problem");
        sqlx::query("INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)")
            .bind(contest_id).bind(problem_id).execute(&pool).await
            .expect("assign clarification problem");
        let team = AuthUser {
            id: team_user_id,
            username: "clar-team".into(),
            display_name: "Clar Team".into(),
            user_type: UserType::Team,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let other = AuthUser {
            id: other_user_id,
            username: "clar-other".into(),
            display_name: "Other Team".into(),
            user_type: UserType::Team,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let admin = AuthUser {
            id: admin_id,
            username: "clar-root".into(),
            display_name: "Clar Root".into(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let service = ClarificationService::new(pool.clone());
        let asked = service
            .ask(
                contest_id,
                AskRequest {
                    scope: "PROBLEM".into(),
                    problem_id: Some(problem_id),
                    question: "Is input sorted?".into(),
                }
                .validate()
                .expect("valid clarification"),
                &team,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("ask clarification");
        assert_eq!(asked.problem_alias.as_deref(), Some("A"));
        assert_eq!(service.list_mine(contest_id, &team).await.expect("list mine").len(), 1);
        assert!(service.list_mine(contest_id, &other).await.expect("list other").is_empty());
        assert!(
            service
                .ask(
                    contest_id,
                    AskRequest {
                        scope: "GENERAL".into(),
                        problem_id: None,
                        question: "Second".into()
                    }
                    .validate()
                    .expect("valid second clarification"),
                    &team,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                )
                .await
                .is_err()
        );
        let replied = service
            .reply(
                asked.id,
                ReplyRequest { reply: "No.".into(), visibility: "PRIVATE".into() }
                    .validate()
                    .expect("valid private reply"),
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("reply privately");
        assert_eq!(replied.reply_visibility.as_deref(), Some("PRIVATE"));
        service
            .close(asked.id, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("close clarification");
        assert!(
            service
                .reply(
                    asked.id,
                    ReplyRequest { reply: "Changed".into(), visibility: "PUBLIC".into() }
                        .validate()
                        .expect("valid later reply"),
                    &admin,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                )
                .await
                .is_err()
        );
        let team_recipients = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT team_id
            FROM realtime_outbox
            WHERE contest_id = $1 AND event_type = 'CLARIFICATION_UPDATED' AND scope = 'TEAM'
            ORDER BY created_at
        "#,
        )
        .bind(contest_id)
        .fetch_all(&pool)
        .await
        .expect("load team recipients");
        assert_eq!(team_recipients, vec![Some(team_id), Some(team_id), Some(team_id)]);
        let audit_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE target_type = 'CLARIFICATION' AND target_id = $1",
        ).bind(asked.id.to_string()).fetch_one(&pool).await.expect("count clarification audits");
        assert_eq!(audit_count, 3);

        let public_question = service
            .ask(
                contest_id,
                AskRequest {
                    scope: "GENERAL".into(),
                    problem_id: None,
                    question: "What is the rule?".into(),
                }
                .validate()
                .expect("valid public clarification"),
                &other,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("ask public clarification");
        service
            .reply(
                public_question.id,
                ReplyRequest { reply: "The public answer.".into(), visibility: "PUBLIC".into() }
                    .validate()
                    .expect("valid public reply"),
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("reply publicly");
        let announcement = service
            .convert(
                public_question.id,
                ConvertRequest { title: None, body: None },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("convert public clarification");
        assert_eq!(announcement.source_clarification_id, Some(public_question.id));
        assert_eq!(announcement.body, "The public answer.");
        assert!(
            service
                .convert(
                    public_question.id,
                    ConvertRequest { title: None, body: None },
                    &admin,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                )
                .await
                .is_err()
        );
        let linked = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT converted_announcement_id FROM clarifications WHERE id = $1",
        )
        .bind(public_question.id)
        .fetch_one(&pool)
        .await
        .expect("load converted link");
        assert_eq!(linked, Some(announcement.id));
    }
}
