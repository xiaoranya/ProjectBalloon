use std::{net::IpAddr, process::Stdio, time::Duration};

use bytes::Bytes;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::{io::AsyncWriteExt, process::Command};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    object_storage::ObjectStorageHandle,
    object_storage_cleanup::defer_failed_cleanup,
    state::AppState,
};

use super::model::{PrintRequestResponse, ValidatedContent};

pub struct PrintingService {
    database: PgPool,
}

impl PrintingService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(super) async fn create(
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

    pub(super) async fn list_mine(
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
        sqlx::query_as::<_, PrintRequestResponse>(safe_sql!("{PRINT_REQUEST_SQL} JOIN team_accounts account ON account.team_id = request.team_id WHERE request.contest_id = $1 AND account.user_id = $2 ORDER BY request.created_at DESC LIMIT 100"))
            .bind(contest_id).bind(actor.id).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list team print requests", error))
    }

    pub(super) async fn list_all(
        &self,
        contest_id: i64,
        status: Option<String>,
        actor: &AuthUser,
    ) -> Result<Vec<PrintRequestResponse>, AppError> {
        require_operator(actor)?;
        sqlx::query_as::<_, PrintRequestResponse>(safe_sql!("{PRINT_REQUEST_SQL} WHERE request.contest_id = $1 AND ($2::text IS NULL OR request.status = $2) ORDER BY request.created_at DESC LIMIT 1000"))
            .bind(contest_id).bind(status.as_deref()).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list print queue", error))
    }

    pub(super) async fn transition(
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
                r#"
                SELECT request.contest_id, request.team_id, request.status,
                       coalesce(request.delivery_lease_until > now(), false)
                FROM print_requests request
                JOIN contests contest
                    ON contest.id = request.contest_id AND contest.deleted_at IS NULL
                WHERE request.id = $1
                FOR UPDATE OF request
                "#,
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
        sqlx::query(
            r#"
            UPDATE print_requests
            SET status = $2,
                failed_reason = $3,
                operator_user_id = $4,
                cancellation_pending = CASE WHEN $5 = 'CANCEL' AND cups_job_id IS NOT NULL
                    THEN true ELSE false END,
                printer_id = CASE WHEN $5 = 'RETRY' THEN NULL ELSE printer_id END,
                cups_job_id = CASE WHEN $5 = 'RETRY' THEN NULL ELSE cups_job_id END,
                submitted_at = CASE WHEN $5 = 'RETRY' THEN NULL ELSE submitted_at END,
                delivery_attempts = CASE WHEN $5 = 'RETRY' THEN 0 ELSE delivery_attempts END,
                delivery_lease_owner = CASE WHEN $5 = 'RETRY' THEN NULL ELSE delivery_lease_owner END,
                delivery_lease_until = CASE WHEN $5 = 'RETRY' THEN NULL ELSE delivery_lease_until END,
                last_delivery_error = NULL,
                updated_at = now(),
                version = version + 1
            WHERE id = $1
            "#,
        )
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

    pub(super) async fn pdf(
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

const PRINT_REQUEST_SQL: &str = r#"SELECT request.id, request.contest_id, request.team_id, request.team_name,
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
    sqlx::query_as::<_, PrintRequestResponse>(safe_sql!(
        "{PRINT_REQUEST_SQL} WHERE request.id = $1"
    ))
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
        .ok_or_else(|| AppError::internal_message("open PDF renderer stdin", "missing pipe"))?;
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

pub(super) fn storage(state: &AppState) -> Result<&ObjectStorageHandle, AppError> {
    state.object_storage().ok_or_else(|| {
        AppError::service_unavailable(
            "OBJECT_STORAGE_UNAVAILABLE",
            "Object storage is not configured",
        )
    })
}
