use std::{
    net::{IpAddr, SocketAddr},
    process::Stdio,
    time::Duration,
};

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use tokio::{io::AsyncWriteExt, process::Command};
use utoipa::ToSchema;
use uuid::Uuid;

mod delivery;
pub use delivery::{CommandLineCupsGateway, CupsDeliveryRunner, CupsGateway, CupsJobStatus};

use crate::{
    error::AppError,
    features::auth::{
        AuthContext,
        model::{AuthUser, UserType},
    },
    object_storage::ObjectStorageHandle,
    object_storage_cleanup::defer_failed_cleanup,
    state::AppState,
};

const MAX_CONTENT_BYTES: usize = 20 * 1024;
const MAX_PAGES: usize = 5;
const LINES_PER_PAGE: usize = 50;
const COLUMNS_PER_LINE: usize = 100;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    content: String,
}

struct ValidatedContent {
    content: String,
    page_count: i32,
    hash: String,
}

impl CreateRequest {
    fn validate(self) -> Result<ValidatedContent, AppError> {
        let content = self.content.replace("\r\n", "\n").replace('\r', "\n");
        if content.trim().is_empty() || content.len() > MAX_CONTENT_BYTES {
            return Err(AppError::validation("content", "must contain 1 byte to 20 KiB"));
        }
        if content
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        {
            return Err(AppError::validation("content", "contains unsupported control characters"));
        }
        let page_count = estimate_pages(&content);
        if page_count > MAX_PAGES {
            return Err(AppError::validation("content", "must fit within 5 estimated A4 pages"));
        }
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        Ok(ValidatedContent {
            content,
            page_count: i32::try_from(page_count)
                .map_err(|error| AppError::internal("convert print page count", error))?,
            hash,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RejectRequest {
    reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    status: Option<String>,
}

impl ListQuery {
    fn validate(self) -> Result<Option<String>, AppError> {
        self.status
            .map(|value| {
                let value = value.trim().to_ascii_uppercase();
                if matches!(
                    value.as_str(),
                    "REQUESTED"
                        | "QUEUED"
                        | "PRINTING"
                        | "COMPLETED"
                        | "FAILED"
                        | "CANCELLED"
                        | "REJECTED"
                ) {
                    Ok(value)
                } else {
                    Err(AppError::validation("status", "contains an unsupported print status"))
                }
            })
            .transpose()
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PrintRequestResponse {
    id: i64,
    contest_id: i64,
    team_id: i64,
    team_name: Option<String>,
    seat_no: Option<String>,
    content_hash: String,
    page_count: i32,
    status: String,
    printer_id: Option<String>,
    cups_job_id: Option<String>,
    requested_by_user_id: i64,
    operator_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    completed_at: Option<OffsetDateTime>,
    failed_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    version: i32,
}

pub struct PrintingService {
    database: PgPool,
}

impl PrintingService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn create(
        &self,
        contest_id: i64,
        command: ValidatedContent,
        actor: &AuthUser,
        ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<PrintRequestResponse, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can request printing",
            ));
        }
        let (team_id, _, _) = resolve_team(&self.database, contest_id, actor.id).await?;
        check_print_quota(&self.database, contest_id, team_id).await?;
        let pdf = render_pdf(&command.content).await?;
        let object_key = format!("prints/{contest_id}/{team_id}/{}.pdf", Uuid::new_v4());
        storage
            .backend()
            .put(storage.source_bucket(), &object_key, Some("application/pdf"), pdf)
            .await
            .map_err(|error| AppError::internal("upload print PDF", error))?;
        let persisted = self
            .persist(contest_id, team_id, command, &object_key, actor, ip, storage.source_bucket())
            .await;
        if persisted.is_err()
            && let Err(cleanup_error) =
                storage.backend().delete(storage.source_bucket(), &object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.source_bucket(),
                &object_key,
                "PRINT_PDF_UPLOAD_COMPENSATION",
                cleanup_error.to_string(),
            )
            .await;
        }
        persisted
    }

    // Persistence inputs mirror the print request and its audit fields.
    #[allow(clippy::too_many_arguments)]
    async fn persist(
        &self,
        contest_id: i64,
        expected_team_id: i64,
        command: ValidatedContent,
        object_key: &str,
        actor: &AuthUser,
        ip: IpAddr,
        bucket: &str,
    ) -> Result<PrintRequestResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin print request", error))?;
        let (team_id, team_name, seat_no) = resolve_team_tx(&mut tx, contest_id, actor.id).await?;
        if team_id != expected_team_id {
            return Err(print_not_found());
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("printing:{contest_id}:{team_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("lock print quota", error))?;
        let (recent, total) = sqlx::query_as::<_, (bool, i64)>(
            r#"
            SELECT EXISTS (SELECT 1 FROM print_requests WHERE contest_id = $1 AND team_id = $2
                           AND created_at > now() - interval '10 minutes'),
                   (SELECT count(*) FROM print_requests WHERE contest_id = $1 AND team_id = $2)
        "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("check print quota", error))?;
        if recent {
            return Err(AppError::too_many_requests(
                "PRINTING_RATE_LIMITED",
                "A team may print once every ten minutes",
            ));
        }
        if total >= 20 {
            return Err(AppError::conflict(
                "PRINTING_QUOTA_EXCEEDED",
                "A team may print at most 20 times per contest",
            ));
        }
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO print_requests
                (contest_id, team_id, team_name, seat_no, content, content_hash, page_count,
                 status, pdf_object_key, pdf_bucket, requested_by, request_ip)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'QUEUED', $8, $9, $10, $11) RETURNING id
        "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .bind(team_name)
        .bind(seat_no)
        .bind(command.content)
        .bind(command.hash)
        .bind(command.page_count)
        .bind(object_key)
        .bind(bucket)
        .bind(actor.id)
        .bind(ip.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("insert print request", error))?;
        audit(&mut tx, actor.id, "PRINTING_REQUESTED", id, ip).await?;
        event(&mut tx, contest_id, team_id, id, "QUEUED").await?;
        tx.commit().await.map_err(|error| AppError::internal("commit print request", error))?;
        load(&self.database, id).await
    }

    async fn list_mine(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<PrintRequestResponse>, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can view team print requests",
            ));
        }
        sqlx::query_as::<_, PrintRequestResponse>(safe_sql!("{SELECT_COLUMNS} JOIN team_accounts account ON account.team_id = request.team_id WHERE request.contest_id = $1 AND account.user_id = $2 ORDER BY request.created_at DESC LIMIT 100"))
            .bind(contest_id).bind(actor.id).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list team print requests", error))
    }

    async fn list_all(
        &self,
        contest_id: i64,
        status: Option<String>,
        actor: &AuthUser,
    ) -> Result<Vec<PrintRequestResponse>, AppError> {
        require_operator(actor)?;
        sqlx::query_as::<_, PrintRequestResponse>(safe_sql!("{SELECT_COLUMNS} WHERE request.contest_id = $1 AND ($2::text IS NULL OR request.status = $2) ORDER BY request.created_at DESC LIMIT 1000"))
            .bind(contest_id).bind(status.as_deref()).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list print queue", error))
    }

    async fn transition(
        &self,
        id: i64,
        action: &'static str,
        actor: &AuthUser,
        ip: IpAddr,
        reason: Option<String>,
    ) -> Result<PrintRequestResponse, AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin print transition", error))?;
        let (contest_id, team_id, status, delivery_in_progress) =
            sqlx::query_as::<_, (i64, i64, String, bool)>(
                "SELECT request.contest_id, request.team_id, request.status, coalesce(request.delivery_lease_until > now(), false) FROM print_requests request JOIN contests contest ON contest.id = request.contest_id AND contest.deleted_at IS NULL WHERE request.id = $1 FOR UPDATE OF request",
            )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock print request", error))?
        .ok_or_else(print_not_found)?;
        if action == "CANCEL" && status == "QUEUED" && delivery_in_progress {
            return Err(AppError::conflict(
                "PRINTING_DELIVERY_IN_PROGRESS",
                "Print request is being submitted to CUPS; retry cancellation shortly",
            ));
        }
        let (next, failed_reason) = match action {
            "RETRY" if matches!(status.as_str(), "FAILED" | "QUEUED") => ("QUEUED", None),
            "CANCEL" if !matches!(status.as_str(), "COMPLETED" | "CANCELLED" | "REJECTED") => {
                ("CANCELLED", None)
            }
            "REJECT" if matches!(status.as_str(), "REQUESTED" | "QUEUED") => {
                let reason = sanitize_reason(reason)?;
                ("REJECTED", Some(reason))
            }
            _ => {
                return Err(AppError::conflict(
                    "PRINTING_STATE_CHANGED",
                    "Print request cannot perform this transition",
                ));
            }
        };
        sqlx::query("UPDATE print_requests SET status = $2, failed_reason = $3, operator_user_id = $4, cancellation_pending = CASE WHEN $5 = 'CANCEL' AND cups_job_id IS NOT NULL THEN true ELSE false END, printer_id = CASE WHEN $5 = 'RETRY' THEN NULL ELSE printer_id END, cups_job_id = CASE WHEN $5 = 'RETRY' THEN NULL ELSE cups_job_id END, submitted_at = CASE WHEN $5 = 'RETRY' THEN NULL ELSE submitted_at END, delivery_attempts = CASE WHEN $5 = 'RETRY' THEN 0 ELSE delivery_attempts END, delivery_lease_owner = CASE WHEN $5 = 'RETRY' THEN NULL ELSE delivery_lease_owner END, delivery_lease_until = CASE WHEN $5 = 'RETRY' THEN NULL ELSE delivery_lease_until END, last_delivery_error = NULL, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(next).bind(failed_reason).bind(actor.id).bind(action).execute(&mut *tx).await
            .map_err(|error| AppError::internal("transition print request", error))?;
        audit(
            &mut tx,
            actor.id,
            match action {
                "RETRY" => "PRINTING_RETRIED",
                "CANCEL" => "PRINTING_CANCELLED",
                _ => "PRINTING_REJECTED",
            },
            id,
            ip,
        )
        .await?;
        event(&mut tx, contest_id, team_id, id, next).await?;
        tx.commit().await.map_err(|error| AppError::internal("commit print transition", error))?;
        load(&self.database, id).await
    }

    async fn pdf(
        &self,
        id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<Bytes, AppError> {
        let (team_id, bucket, key) = sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
            "SELECT request.team_id, request.pdf_bucket, request.pdf_object_key FROM print_requests request JOIN contests contest ON contest.id = request.contest_id AND contest.deleted_at IS NULL WHERE request.id = $1",
        )
        .bind(id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load print PDF metadata", error))?
        .ok_or_else(print_not_found)?;
        if !actor.is_super_admin()
            && !actor.has_permission(crate::features::auth::permissions::PRINTING_MANAGE)
        {
            let own = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM team_accounts WHERE user_id = $1 AND team_id = $2)",
            )
            .bind(actor.id)
            .bind(team_id)
            .fetch_one(&self.database)
            .await
            .map_err(|error| AppError::internal("check print PDF owner", error))?;
            if !own {
                return Err(print_not_found());
            }
        }
        let bucket = bucket.ok_or_else(|| {
            AppError::conflict("PRINTING_PDF_NOT_READY", "Print PDF is not ready")
        })?;
        let key = key.ok_or_else(|| {
            AppError::conflict("PRINTING_PDF_NOT_READY", "Print PDF is not ready")
        })?;
        storage
            .backend()
            .get(&bucket, &key)
            .await
            .map_err(|error| AppError::internal("download print PDF", error))
    }
}

const SELECT_COLUMNS: &str = r#"SELECT request.id, request.contest_id, request.team_id, request.team_name,
 request.seat_no, request.content_hash, request.page_count, request.status, request.printer_id,
 request.cups_job_id, request.requested_by AS requested_by_user_id, request.operator_user_id,
 request.completed_at, request.failed_reason, request.created_at, request.updated_at, request.version
 FROM print_requests request
 JOIN contests contest ON contest.id = request.contest_id AND contest.deleted_at IS NULL"#;

async fn resolve_team(
    database: &PgPool,
    contest_id: i64,
    user_id: i64,
) -> Result<(i64, String, Option<String>), AppError> {
    let mut tx = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin print team lookup", error))?;
    let row = resolve_team_tx(&mut tx, contest_id, user_id).await?;
    tx.commit().await.map_err(|error| AppError::internal("commit print team lookup", error))?;
    Ok(row)
}

async fn resolve_team_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    user_id: i64,
) -> Result<(i64, String, Option<String>), AppError> {
    sqlx::query_as(
        r#"SELECT team.id, team.name, team.seat_no FROM team_accounts account
        JOIN teams team ON team.id = account.team_id AND team.deleted_at IS NULL
        JOIN contest_teams roster ON roster.team_id = team.id AND roster.contest_id = $2
        JOIN contests contest ON contest.id = roster.contest_id AND contest.deleted_at IS NULL
          AND contest.status IN ('RUNNING', 'PAUSED') WHERE account.user_id = $1"#,
    )
    .bind(user_id)
    .bind(contest_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| AppError::internal("resolve print team", error))?
    .ok_or_else(print_not_found)
}

async fn load(database: &PgPool, id: i64) -> Result<PrintRequestResponse, AppError> {
    sqlx::query_as::<_, PrintRequestResponse>(safe_sql!("{SELECT_COLUMNS} WHERE request.id = $1"))
        .bind(id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("load print request", error))?
        .ok_or_else(print_not_found)
}

async fn check_print_quota(
    database: &PgPool,
    contest_id: i64,
    team_id: i64,
) -> Result<(), AppError> {
    let (recent, total) = sqlx::query_as::<_, (bool, i64)>(
        "SELECT EXISTS (SELECT 1 FROM print_requests WHERE contest_id=$1 AND team_id=$2 AND created_at > now()-interval '10 minutes'), (SELECT count(*) FROM print_requests WHERE contest_id=$1 AND team_id=$2)",
    )
    .bind(contest_id)
    .bind(team_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check print quota before rendering", error))?;
    if recent {
        return Err(AppError::too_many_requests(
            "PRINTING_RATE_LIMITED",
            "A team may print once every ten minutes",
        ));
    }
    if total >= 20 {
        return Err(AppError::conflict(
            "PRINTING_QUOTA_EXCEEDED",
            "A team may print at most 20 times per contest",
        ));
    }
    Ok(())
}

fn estimate_pages(content: &str) -> usize {
    let lines = content
        .split('\n')
        .map(|line| {
            let columns =
                line.chars().map(|character| if character == '\t' { 4 } else { 1 }).sum::<usize>();
            columns.max(1).div_ceil(COLUMNS_PER_LINE)
        })
        .sum::<usize>();
    lines.max(1).div_ceil(LINES_PER_PAGE)
}

async fn render_pdf(content: &str) -> Result<Bytes, AppError> {
    const GENERIC_PDF_PPD: &str = "/usr/share/ppd/cupsfilters/Generic-PDF_Printer-PDF.ppd";
    if !std::path::Path::new(GENERIC_PDF_PPD).is_file() {
        return Err(AppError::service_unavailable(
            "PDF_RENDERER_UNAVAILABLE",
            "generic CUPS PDF profile is not installed",
        ));
    }
    let mut child = Command::new("cupsfilter")
        .args([
            "-p",
            GENERIC_PDF_PPD,
            "-i",
            "text/plain",
            "-m",
            "application/pdf",
            "-o",
            "media=A4",
            "-o",
            "emit-jcl=false",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            AppError::service_unavailable(
                "PDF_RENDERER_UNAVAILABLE",
                if error.kind() == std::io::ErrorKind::NotFound {
                    "cupsfilter is not installed"
                } else {
                    "PDF renderer could not start"
                },
            )
        })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::internal("open PDF renderer stdin", "missing pipe"))?;
    stdin
        .write_all(content.as_bytes())
        .await
        .map_err(|error| AppError::internal("write PDF renderer input", error))?;
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .map_err(|_| {
            AppError::service_unavailable("PDF_RENDERER_TIMEOUT", "PDF rendering timed out")
        })?
        .map_err(|error| AppError::internal("wait for PDF renderer", error))?;
    if !output.status.success()
        || !output.stdout.starts_with(b"%PDF-")
        || output.stdout.len() > 5 * 1024 * 1024
    {
        return Err(AppError::service_unavailable("PDF_RENDER_FAILED", "PDF rendering failed"));
    }
    Ok(Bytes::from(output.stdout))
}

fn require_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::PRINTING_MANAGE)
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "PRINTING_PERMISSION_REQUIRED",
            "Printing management permission is required",
        ))
    }
}

fn sanitize_reason(reason: Option<String>) -> Result<String, AppError> {
    let reason = reason
        .unwrap_or_default()
        .replace(['\r', '\n'], " ")
        .trim()
        .chars()
        .take(255)
        .collect::<String>();
    if reason.is_empty() {
        Err(AppError::validation("reason", "must not be empty"))
    } else {
        Ok(reason)
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'PRINT_REQUEST', $3, $4, 'success')")
        .bind(actor).bind(action).bind(id.to_string()).bind(ip.to_string()).execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("record print audit", error))
}

async fn event(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
    team: i64,
    id: i64,
    action: &str,
) -> Result<(), AppError> {
    for (scope, recipient) in [("STAFF", None), ("TEAM", Some(team))] {
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, team_id, payload_json) VALUES ($1, $2, 'PRINT_REQUEST_UPDATED', $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(contest).bind(scope).bind(recipient).bind(json!({"printRequestId": id, "action": action}))
            .execute(&mut **tx).await.map_err(|error| AppError::internal("enqueue print event", error))?;
    }
    Ok(())
}

fn print_not_found() -> AppError {
    AppError::not_found("PRINT_REQUEST_NOT_FOUND", "Print request was not found")
}

fn storage(state: &AppState) -> Result<&ObjectStorageHandle, AppError> {
    state.object_storage().ok_or_else(|| {
        AppError::service_unavailable(
            "OBJECT_STORAGE_UNAVAILABLE",
            "Object storage is not configured",
        )
    })
}

#[utoipa::path(post, path = "/api/contests/{contest_id}/print-requests", operation_id = "createPrintRequest", tag = "printing",
    params(("contest_id" = i64, Path)), request_body = CreateRequest,
    responses((status = 201, body = PrintRequestResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 429, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PrintRequestResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain printable text"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .printing()
                .create(
                    contest_id,
                    request.validate()?,
                    context.user(),
                    peer.ip(),
                    storage(&state)?,
                )
                .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/print-requests/mine", operation_id = "listOwnPrintRequests", tag = "printing",
    params(("contest_id" = i64, Path)), responses((status = 200, body = [PrintRequestResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_mine(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<PrintRequestResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().list_mine(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/print-requests/all", operation_id = "listAllPrintRequests", tag = "printing",
    params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [PrintRequestResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_all(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Vec<PrintRequestResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid print filters"))?;
    Ok(Json(state.printing().list_all(contest_id, query.validate()?, context.user()).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/retry", operation_id = "retryPrintRequest", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = PrintRequestResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn retry(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().transition(id, "RETRY", context.user(), peer.ip(), None).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/cancel", operation_id = "cancelPrintRequest", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = PrintRequestResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn cancel(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().transition(id, "CANCEL", context.user(), peer.ip(), None).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/reject", operation_id = "rejectPrintRequest", tag = "printing",
    params(("id" = i64, Path)), request_body = RejectRequest, responses((status = 200, body = PrintRequestResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reject(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<RejectRequest>, JsonRejection>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain a rejection reason"))?;
    Ok(Json(
        state
            .printing()
            .transition(id, "REJECT", context.user(), peer.ip(), Some(request.reason))
            .await?,
    ))
}

#[utoipa::path(get, path = "/api/print-requests/{id}/pdf", operation_id = "downloadPrintPdf", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = Vec<u8>, content_type = "application/pdf"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn download_pdf(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    let pdf = state.printing().pdf(id, context.user(), storage(&state)?).await?;
    let disposition = HeaderValue::from_str(&format!("attachment; filename=print-{id}.pdf"))
        .map_err(|error| AppError::internal("build print filename", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/pdf")),
            (header::CONTENT_DISPOSITION, disposition),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        ],
        pdf,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use sqlx::PgPool;

    use super::{CreateRequest, PrintingService, estimate_pages};
    use crate::{
        features::auth::model::{AuthUser, UserType},
        object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
    };

    #[derive(Default)]
    struct MemoryStorage {
        objects: Mutex<HashMap<(String, String), Bytes>>,
    }

    #[async_trait]
    impl ObjectStorage for MemoryStorage {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }
        async fn put(
            &self,
            bucket: &str,
            key: &str,
            _content_type: Option<&str>,
            content: Bytes,
        ) -> Result<(), ObjectStorageError> {
            self.objects.lock().expect("storage lock").insert((bucket.into(), key.into()), content);
            Ok(())
        }
        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
            self.objects
                .lock()
                .expect("storage lock")
                .get(&(bucket.into(), key.into()))
                .cloned()
                .ok_or_else(|| ObjectStorageError::Request("not found".into()))
        }
        async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
            self.objects.lock().expect("storage lock").remove(&(bucket.into(), key.into()));
            Ok(())
        }
    }

    #[test]
    fn print_content_size_controls_and_page_estimate_are_closed() {
        assert_eq!(estimate_pages("hello"), 1);
        assert_eq!(estimate_pages(&"line\n".repeat(50)), 2);
        assert!(CreateRequest { content: "ok".into() }.validate().is_ok());
        assert!(CreateRequest { content: "\0".into() }.validate().is_err());
        assert!(CreateRequest { content: "x".repeat(20 * 1024 + 1) }.validate().is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL and the cupsfilter executable"]
    async fn print_request_renders_archives_limits_and_hides_other_teams(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-root', 'hash', 'Print Root', 'SUPER_ADMIN') RETURNING id")
            .fetch_one(&pool).await.expect("insert print administrator");
        let team_user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-team', 'hash', 'Print Team', 'TEAM') RETURNING id")
            .fetch_one(&pool).await.expect("insert print team user");
        let other_user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-other', 'hash', 'Print Other', 'TEAM') RETURNING id")
            .fetch_one(&pool).await.expect("insert other print user");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name, seat_no) VALUES ('Print Team', 'A01') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert print team");
        let other_team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Print Other') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert other print team");
        for (user, team) in [(team_user_id, team_id), (other_user_id, other_team_id)] {
            sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
                .bind(user)
                .bind(team)
                .execute(&pool)
                .await
                .expect("link print team");
        }
        let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, end_at) VALUES ('Print Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour') RETURNING id")
            .fetch_one(&pool).await.expect("insert print contest");
        for team in [team_id, other_team_id] {
            sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
                .bind(contest_id).bind(team).execute(&pool).await.expect("roster print team");
        }
        let team = AuthUser {
            id: team_user_id,
            username: "print-team".into(),
            display_name: "Print Team".into(),
            user_type: UserType::Team,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        let other = AuthUser {
            id: other_user_id,
            username: "print-other".into(),
            display_name: "Print Other".into(),
            user_type: UserType::Team,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        let admin = AuthUser {
            id: admin_id,
            username: "print-root".into(),
            display_name: "Print Root".into(),
            user_type: UserType::SuperAdmin,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        let storage = ObjectStorageHandle::with_buckets(
            Arc::new(MemoryStorage::default()),
            "problems".into(),
            "artifacts".into(),
        );
        let service = PrintingService::new(pool.clone());
        let created = service
            .create(
                contest_id,
                CreateRequest { content: "hello printer\n".into() }
                    .validate()
                    .expect("valid print"),
                &team,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("create print request");
        assert_eq!(created.status, "QUEUED");
        assert_eq!(created.page_count, 1);
        let pdf = service.pdf(created.id, &team, &storage).await.expect("download own PDF");
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(service.pdf(created.id, &other, &storage).await.is_err());
        assert!(
            service
                .create(
                    contest_id,
                    CreateRequest { content: "again".into() }
                        .validate()
                        .expect("valid second print"),
                    &team,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    &storage
                )
                .await
                .is_err()
        );
        let rejected = service
            .transition(
                created.id,
                "REJECT",
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                Some("Not printable".into()),
            )
            .await
            .expect("reject print");
        assert_eq!(rejected.status, "REJECTED");
        assert_eq!(rejected.failed_reason.as_deref(), Some("Not printable"));
        assert!(
            service.list_mine(contest_id, &other).await.expect("list other print jobs").is_empty()
        );
    }
}
