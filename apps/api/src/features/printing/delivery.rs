use std::{process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use tokio::{io::AsyncWriteExt, process::Command, sync::watch};
use tracing::{info, warn};
use uuid::Uuid;

use crate::object_storage::ObjectStorageHandle;

const DELIVERY_LEASE_SECONDS: i32 = 10;
const MONITOR_DELAY_SECONDS: i32 = 5;
const UNKNOWN_JOB_GRACE_MINUTES: i32 = 30;
const MAX_DELIVERY_ATTEMPTS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CupsJobStatus {
    Pending,
    Completed,
    Unknown,
}

#[async_trait]
pub trait CupsGateway: Send + Sync {
    async fn probe(&self) -> Result<(), String>;
    async fn submit(&self, title: &str, pdf: Bytes) -> Result<String, String>;
    async fn status(&self, job_id: &str) -> Result<CupsJobStatus, String>;
    async fn cancel(&self, job_id: &str) -> Result<(), String>;
    fn printer(&self) -> &str;
}

#[derive(Clone)]
pub struct CommandLineCupsGateway {
    printer: String,
    timeout: Duration,
}

impl CommandLineCupsGateway {
    #[must_use]
    pub const fn new(printer: String, timeout: Duration) -> Self {
        Self { printer, timeout }
    }

    async fn execute(
        &self,
        program: &str,
        arguments: &[&str],
        input: Option<Bytes>,
    ) -> Result<std::process::Output, String> {
        let mut command = Command::new(program);
        command
            .args(arguments)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("{program} is not installed")
            } else {
                sanitize_error(&error.to_string())
            }
        })?;
        if let Some(input) = input {
            let mut stdin = child.stdin.take().ok_or_else(|| "missing command stdin".to_owned())?;
            stdin.write_all(&input).await.map_err(|error| sanitize_error(&error.to_string()))?;
            drop(stdin);
        }
        tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| format!("{program} timed out"))?
            .map_err(|error| sanitize_error(&error.to_string()))
    }

    async fn checked(
        &self,
        program: &str,
        arguments: &[&str],
        input: Option<Bytes>,
    ) -> Result<std::process::Output, String> {
        let output = self.execute(program, arguments, input).await?;
        if output.status.success() {
            Ok(output)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(if stderr.trim().is_empty() {
                format!("{program} exited with {}", output.status)
            } else {
                sanitize_error(&stderr)
            })
        }
    }
}

#[async_trait]
impl CupsGateway for CommandLineCupsGateway {
    async fn probe(&self) -> Result<(), String> {
        self.checked("lpstat", &["-p", &self.printer], None).await.map(|_| ())
    }

    async fn submit(&self, title: &str, pdf: Bytes) -> Result<String, String> {
        let output = self
            .checked("lp", &["-d", &self.printer, "-t", title, "-o", "media=A4", "-"], Some(pdf))
            .await?;
        parse_job_id(&String::from_utf8_lossy(&output.stdout), &self.printer)
            .ok_or_else(|| "CUPS did not return a job identifier".to_owned())
    }

    async fn status(&self, job_id: &str) -> Result<CupsJobStatus, String> {
        let active =
            self.checked("lpstat", &["-W", "not-completed", "-o", &self.printer], None).await?;
        if contains_job(&active.stdout, job_id) {
            return Ok(CupsJobStatus::Pending);
        }
        let completed =
            self.checked("lpstat", &["-W", "completed", "-o", &self.printer], None).await?;
        Ok(if contains_job(&completed.stdout, job_id) {
            CupsJobStatus::Completed
        } else {
            CupsJobStatus::Unknown
        })
    }

    async fn cancel(&self, job_id: &str) -> Result<(), String> {
        self.checked("cancel", &[job_id], None).await.map(|_| ())
    }

    fn printer(&self) -> &str {
        &self.printer
    }
}

fn parse_job_id(output: &str, printer: &str) -> Option<String> {
    let prefix = format!("{printer}-");
    output
        .split_whitespace()
        .find(|token| {
            token.starts_with(&prefix) && token[prefix.len()..].chars().all(|c| c.is_ascii_digit())
        })
        .map(|token| {
            token
                .trim_matches(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '-' && character != '_'
                })
                .to_owned()
        })
}

fn contains_job(output: &[u8], job_id: &str) -> bool {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .any(|candidate| candidate == job_id)
}

fn sanitize_error(message: &str) -> String {
    let sanitized = message.replace(['\r', '\n'], " ");
    let sanitized = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    sanitized.chars().take(255).collect()
}

#[derive(sqlx::FromRow)]
struct ClaimedPrintRequest {
    id: i64,
    contest_id: i64,
    team_id: i64,
    team_name: Option<String>,
    status: String,
    cups_job_id: Option<String>,
    pdf_bucket: Option<String>,
    pdf_object_key: Option<String>,
    delivery_attempts: i32,
    submitted_at: Option<OffsetDateTime>,
    cancellation_pending: bool,
}

pub struct CupsDeliveryRunner {
    database: PgPool,
    storage: ObjectStorageHandle,
    gateway: Arc<dyn CupsGateway>,
    instance_id: Uuid,
}

impl CupsDeliveryRunner {
    #[must_use]
    pub fn new(
        database: PgPool,
        storage: ObjectStorageHandle,
        gateway: Arc<dyn CupsGateway>,
    ) -> Self {
        Self { database, storage, gateway, instance_id: Uuid::new_v4() }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!(printer = self.gateway.printer(), instance_id = %self.instance_id, "CUPS delivery runner started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.claim().await {
                Ok(Some(request)) => self.process(request).await,
                Ok(None) => {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                        result = shutdown.changed() => {
                            if result.is_err() || *shutdown.borrow() { break; }
                        }
                    }
                }
                Err(error) => {
                    warn!(%error, "failed to claim a print request");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        info!(instance_id = %self.instance_id, "CUPS delivery runner stopped");
    }

    async fn claim(&self) -> Result<Option<ClaimedPrintRequest>, sqlx::Error> {
        sqlx::query_as::<_, ClaimedPrintRequest>(
            r#"
            WITH candidate AS (
                SELECT id FROM print_requests
                WHERE (cancellation_pending OR status IN ('QUEUED', 'PRINTING'))
                  AND (delivery_lease_until IS NULL OR delivery_lease_until < now())
                ORDER BY cancellation_pending DESC, created_at, id
                FOR UPDATE SKIP LOCKED LIMIT 1
            )
            UPDATE print_requests request
            SET delivery_lease_owner = $1,
                delivery_lease_until = now() + make_interval(secs => $2),
                delivery_attempts = CASE
                    WHEN request.status = 'QUEUED' AND NOT request.cancellation_pending
                    THEN request.delivery_attempts + 1 ELSE request.delivery_attempts END,
                updated_at = now()
            FROM candidate WHERE request.id = candidate.id
            RETURNING request.id, request.contest_id, request.team_id, request.team_name,
                request.status, request.cups_job_id, request.pdf_bucket,
                request.pdf_object_key, request.delivery_attempts, request.submitted_at,
                request.cancellation_pending
            "#,
        )
        .bind(self.instance_id)
        .bind(DELIVERY_LEASE_SECONDS)
        .fetch_optional(&self.database)
        .await
    }

    async fn process(&self, request: ClaimedPrintRequest) {
        if request.cancellation_pending {
            self.process_cancellation(&request).await;
        } else if request.status == "QUEUED" {
            self.process_queued(&request).await;
        } else if request.status == "PRINTING" {
            self.process_printing(&request).await;
        }
    }

    async fn process_cancellation(&self, request: &ClaimedPrintRequest) {
        let Some(job_id) = request.cups_job_id.as_deref() else {
            self.fail_internal(request, "cancelled print request has no CUPS job identifier").await;
            return;
        };
        match self.gateway.cancel(job_id).await {
            Ok(()) => {
                if let Err(error) = sqlx::query("UPDATE print_requests SET cancellation_pending = false, delivery_lease_owner = NULL, delivery_lease_until = NULL, last_delivery_error = NULL, updated_at = now(), version = version + 1 WHERE id = $1 AND delivery_lease_owner = $2")
                    .bind(request.id).bind(self.instance_id).execute(&self.database).await
                {
                    warn!(print_request_id = request.id, %error, "failed to persist CUPS cancellation");
                }
            }
            Err(error) => self.defer(request.id, request.delivery_attempts, &error).await,
        }
    }

    async fn process_queued(&self, request: &ClaimedPrintRequest) {
        let (Some(bucket), Some(key)) =
            (request.pdf_bucket.as_deref(), request.pdf_object_key.as_deref())
        else {
            self.fail_delivery(request, "print PDF metadata is missing").await;
            return;
        };
        let pdf = match self.storage.backend().get(bucket, key).await {
            Ok(pdf) if pdf.starts_with(b"%PDF-") => pdf,
            Ok(_) => {
                self.fail_delivery(request, "stored print document is not a PDF").await;
                return;
            }
            Err(error) => {
                warn!(print_request_id = request.id, %error, "failed to download print PDF");
                self.fail_delivery(request, "Print PDF is temporarily unavailable").await;
                return;
            }
        };
        let title =
            format!("XCPC {} #{}", request.team_name.as_deref().unwrap_or("team"), request.id);
        match self.gateway.submit(&title, pdf).await {
            Ok(job_id) => {
                if let Err(error) = self.mark_printing(request, &job_id).await {
                    warn!(print_request_id = request.id, cups_job_id = %job_id, %error, "CUPS accepted job but database update failed");
                }
            }
            Err(error) => self.fail_delivery(request, &error).await,
        }
    }

    async fn mark_printing(
        &self,
        request: &ClaimedPrintRequest,
        job_id: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.database.begin().await?;
        let updated = sqlx::query("UPDATE print_requests SET status = 'PRINTING', printer_id = $3, cups_job_id = $4, submitted_at = now(), delivery_lease_owner = NULL, delivery_lease_until = NULL, last_delivery_error = NULL, updated_at = now(), version = version + 1 WHERE id = $1 AND delivery_lease_owner = $2 AND status = 'QUEUED'")
            .bind(request.id).bind(self.instance_id).bind(self.gateway.printer()).bind(job_id)
            .execute(&mut *tx).await?.rows_affected();
        if updated == 1 {
            insert_events(&mut tx, request, "PRINTING").await?;
        }
        tx.commit().await
    }

    async fn process_printing(&self, request: &ClaimedPrintRequest) {
        let Some(job_id) = request.cups_job_id.as_deref() else {
            self.fail_internal(request, "printing request has no CUPS job identifier").await;
            return;
        };
        match self.gateway.status(job_id).await {
            Ok(CupsJobStatus::Pending) => {
                self.defer(request.id, request.delivery_attempts, "").await
            }
            Ok(CupsJobStatus::Completed) => {
                if let Err(error) = self.mark_completed(request).await {
                    warn!(print_request_id = request.id, %error, "failed to complete print request");
                }
            }
            Ok(CupsJobStatus::Unknown) => {
                let expired = request.submitted_at.is_some_and(|submitted| {
                    OffsetDateTime::now_utc() - submitted
                        >= time::Duration::minutes(i64::from(UNKNOWN_JOB_GRACE_MINUTES))
                });
                if expired {
                    self.fail_internal(request, "CUPS job disappeared before completion").await;
                } else {
                    self.defer(
                        request.id,
                        request.delivery_attempts,
                        "CUPS job is temporarily unknown",
                    )
                    .await;
                }
            }
            Err(error) => self.defer(request.id, request.delivery_attempts, &error).await,
        }
    }

    async fn mark_completed(&self, request: &ClaimedPrintRequest) -> Result<(), sqlx::Error> {
        let mut tx = self.database.begin().await?;
        let updated = sqlx::query("UPDATE print_requests SET status = 'COMPLETED', completed_at = now(), failed_reason = NULL, delivery_lease_owner = NULL, delivery_lease_until = NULL, last_delivery_error = NULL, updated_at = now(), version = version + 1 WHERE id = $1 AND delivery_lease_owner = $2 AND status = 'PRINTING'")
            .bind(request.id).bind(self.instance_id).execute(&mut *tx).await?.rows_affected();
        if updated == 1 {
            insert_events(&mut tx, request, "COMPLETED").await?;
        }
        tx.commit().await
    }

    async fn fail_delivery(&self, request: &ClaimedPrintRequest, message: &str) {
        let message = sanitize_error(message);
        let terminal = request.delivery_attempts >= MAX_DELIVERY_ATTEMPTS;
        let delay_seconds = delivery_retry_delay(request.delivery_attempts);
        let mut tx = match self.database.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                warn!(print_request_id = request.id, %error, "failed to begin print failure update");
                return;
            }
        };
        let result = sqlx::query("UPDATE print_requests SET status = CASE WHEN $3 THEN 'FAILED' ELSE 'QUEUED' END, failed_reason = CASE WHEN $3 THEN $4 ELSE NULL END, last_delivery_error = $4, delivery_lease_owner = NULL, delivery_lease_until = CASE WHEN $3 THEN NULL ELSE now() + make_interval(secs => $5) END, updated_at = now(), version = version + 1 WHERE id = $1 AND delivery_lease_owner = $2")
            .bind(request.id).bind(self.instance_id).bind(terminal).bind(&message).bind(delay_seconds)
            .execute(&mut *tx).await;
        match result {
            Ok(result) if result.rows_affected() == 1 => {
                if terminal && insert_events(&mut tx, request, "FAILED").await.is_err() {
                    let _ignored = tx.rollback().await;
                    return;
                }
                if let Err(error) = tx.commit().await {
                    warn!(print_request_id = request.id, %error, "failed to commit print failure");
                }
            }
            Ok(_) => {
                let _ignored = tx.rollback().await;
            }
            Err(error) => {
                let _ignored = tx.rollback().await;
                warn!(print_request_id = request.id, %error, "failed to persist print failure");
            }
        }
    }

    async fn fail_internal(&self, request: &ClaimedPrintRequest, message: &str) {
        let message = sanitize_error(message);
        let mut tx = match self.database.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                warn!(print_request_id = request.id, %error, "failed to begin terminal print update");
                return;
            }
        };
        match sqlx::query("UPDATE print_requests SET status = 'FAILED', failed_reason = $3, last_delivery_error = $3, cancellation_pending = false, delivery_lease_owner = NULL, delivery_lease_until = NULL, updated_at = now(), version = version + 1 WHERE id = $1 AND delivery_lease_owner = $2")
            .bind(request.id).bind(self.instance_id).bind(&message).execute(&mut *tx).await
        {
            Ok(result) if result.rows_affected() == 1 => {
                if insert_events(&mut tx, request, "FAILED").await.is_ok() {
                    if let Err(error) = tx.commit().await {
                        warn!(print_request_id = request.id, %error, "failed to commit terminal print failure");
                    }
                } else {
                    let _ignored = tx.rollback().await;
                }
            }
            Ok(_) => { let _ignored = tx.rollback().await; }
            Err(error) => {
                let _ignored = tx.rollback().await;
                warn!(print_request_id = request.id, %error, "failed to persist terminal print failure");
            }
        }
    }

    async fn defer(&self, id: i64, attempts: i32, message: &str) {
        let message = (!message.is_empty()).then(|| sanitize_error(message));
        let delay_seconds = delivery_retry_delay(attempts);
        if let Err(error) = sqlx::query("UPDATE print_requests SET delivery_lease_until = now() + make_interval(secs => $3), last_delivery_error = COALESCE($4, last_delivery_error), updated_at = now() WHERE id = $1 AND delivery_lease_owner = $2")
            .bind(id).bind(self.instance_id).bind(delay_seconds).bind(message)
            .execute(&self.database).await
        {
            warn!(print_request_id = id, %error, "failed to defer print delivery");
        }
    }
}

fn delivery_retry_delay(attempts: i32) -> i32 {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(6);
    i32::try_from(u64::try_from(MONITOR_DELAY_SECONDS.max(1)).unwrap_or(1) * 2_u64.pow(exponent))
        .unwrap_or(i32::MAX)
        .min(300)
}

async fn insert_events(
    tx: &mut Transaction<'_, Postgres>,
    request: &ClaimedPrintRequest,
    action: &str,
) -> Result<(), sqlx::Error> {
    for (scope, team_id) in [("STAFF", None), ("TEAM", Some(request.team_id))] {
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, team_id, payload_json) VALUES ($1, $2, 'PRINT_REQUEST_UPDATED', $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(request.contest_id).bind(scope).bind(team_id)
            .bind(json!({"printRequestId": request.id, "action": action}))
            .execute(&mut **tx).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use async_trait::async_trait;
    use bytes::Bytes;
    use sqlx::PgPool;

    use super::{
        CommandLineCupsGateway, CupsDeliveryRunner, CupsGateway, CupsJobStatus, contains_job,
        parse_job_id, sanitize_error,
    };
    use crate::object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle};

    struct MemoryStorage {
        objects: Mutex<HashMap<(String, String), Bytes>>,
    }

    #[async_trait]
    impl ObjectStorage for MemoryStorage {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }

        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
            self.objects
                .lock()
                .expect("storage lock")
                .get(&(bucket.to_owned(), key.to_owned()))
                .cloned()
                .ok_or_else(|| ObjectStorageError::Request("not found".to_owned()))
        }

        async fn put(
            &self,
            _bucket: &str,
            _key: &str,
            _content_type: Option<&str>,
            _content: Bytes,
        ) -> Result<(), ObjectStorageError> {
            Ok(())
        }

        async fn delete(&self, _bucket: &str, _key: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }
    }

    struct FakeCupsGateway {
        status: Mutex<CupsJobStatus>,
    }

    #[async_trait]
    impl CupsGateway for FakeCupsGateway {
        async fn probe(&self) -> Result<(), String> {
            Ok(())
        }

        async fn submit(&self, _title: &str, pdf: Bytes) -> Result<String, String> {
            if !pdf.starts_with(b"%PDF-") {
                return Err("invalid PDF".to_owned());
            }
            Ok("fake-42".to_owned())
        }

        async fn status(&self, _job_id: &str) -> Result<CupsJobStatus, String> {
            Ok(*self.status.lock().expect("CUPS status lock"))
        }

        async fn cancel(&self, _job_id: &str) -> Result<(), String> {
            Ok(())
        }

        fn printer(&self) -> &str {
            "fake"
        }
    }

    #[test]
    fn cups_output_is_parsed_without_accepting_ambiguous_jobs() {
        assert_eq!(
            parse_job_id("request id is xcpc-42 (1 file(s))", "xcpc").as_deref(),
            Some("xcpc-42")
        );
        assert_eq!(parse_job_id("request id is other-42", "xcpc"), None);
        assert!(contains_job(b"xcpc-42 user 10\n", "xcpc-42"));
        assert!(!contains_job(b"xcpc-420 user 10\n", "xcpc-42"));
        assert!(!sanitize_error(&"x\n".repeat(300)).contains('\n'));
        assert!(sanitize_error(&"x".repeat(300)).len() <= 255);
        let gateway =
            CommandLineCupsGateway::new("xcpc".to_owned(), std::time::Duration::from_secs(1));
        assert_eq!(gateway.printer(), "xcpc");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn delivery_moves_an_archived_pdf_to_completed(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('delivery-team', 'hash', 'Delivery Team', 'TEAM') RETURNING id")
            .fetch_one(&pool).await.expect("insert delivery user");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Delivery Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert delivery team");
        let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, end_at) VALUES ('Delivery Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour') RETURNING id")
            .fetch_one(&pool).await.expect("insert delivery contest");
        let request_id = sqlx::query_scalar::<_, i64>("INSERT INTO print_requests (contest_id, team_id, team_name, content, content_hash, page_count, status, pdf_object_key, pdf_bucket, requested_by) VALUES ($1, $2, 'Delivery Team', 'hello', $3, 1, 'QUEUED', 'prints/one.pdf', 'artifacts', $4) RETURNING id")
            .bind(contest_id).bind(team_id).bind("0".repeat(64)).bind(user_id)
            .fetch_one(&pool).await.expect("insert queued print request");
        let storage = ObjectStorageHandle::with_buckets(
            std::sync::Arc::new(MemoryStorage {
                objects: Mutex::new(HashMap::from([(
                    ("artifacts".to_owned(), "prints/one.pdf".to_owned()),
                    Bytes::from_static(b"%PDF-1.7\n%%EOF"),
                )])),
            }),
            "problems".to_owned(),
            "artifacts".to_owned(),
        );
        let gateway =
            std::sync::Arc::new(FakeCupsGateway { status: Mutex::new(CupsJobStatus::Pending) });
        let runner = CupsDeliveryRunner::new(pool.clone(), storage, gateway.clone());

        let queued = runner.claim().await.expect("claim queued request").expect("queued request");
        runner.process(queued).await;
        let (status, job_id) = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, cups_job_id FROM print_requests WHERE id = $1",
        )
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("load submitted print request");
        assert_eq!(status, "PRINTING");
        assert_eq!(job_id.as_deref(), Some("fake-42"));

        *gateway.status.lock().expect("CUPS status lock") = CupsJobStatus::Completed;
        let printing =
            runner.claim().await.expect("claim printing request").expect("printing request");
        runner.process(printing).await;
        let (status, completed_at, event_count) = sqlx::query_as::<_, (String, Option<time::OffsetDateTime>, i64)>(
            "SELECT status, completed_at, (SELECT count(*) FROM realtime_outbox WHERE payload_json->>'printRequestId' = $1::text) FROM print_requests WHERE id = $1",
        )
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("load completed print request");
        assert_eq!(status, "COMPLETED");
        assert!(completed_at.is_some());
        assert_eq!(event_count, 4);
    }
}
