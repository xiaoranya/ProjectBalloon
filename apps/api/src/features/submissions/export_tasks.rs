use std::{str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError, features::auth::model::AuthUser, object_storage::ObjectStorageHandle,
    object_storage_cleanup::defer_failed_cleanup,
};

use super::{query::require_admin_access, service::SubmissionService};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportTaskKind {
    MetadataCsv,
    SourcesZip,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateExportTaskRequest {
    pub kind: ExportTaskKind,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{ExportTaskKind, retry_delay};
    use crate::features::submissions::SubmissionService;

    #[test]
    fn export_kinds_have_stable_wire_names() {
        assert_eq!(
            serde_json::to_string(&ExportTaskKind::MetadataCsv).expect("serialize metadata kind"),
            "\"METADATA_CSV\""
        );
        assert_eq!(
            serde_json::to_string(&ExportTaskKind::SourcesZip).expect("serialize source kind"),
            "\"SOURCES_ZIP\""
        );
    }

    #[test]
    fn export_retry_delay_is_exponential_and_capped() {
        let base = Duration::from_secs(5);
        assert_eq!(retry_delay(base, 1), Duration::from_secs(5));
        assert_eq!(retry_delay(base, 4), Duration::from_secs(40));
        assert_eq!(retry_delay(base, 100), Duration::from_secs(3_600));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn export_leases_are_owner_bound_and_expired_outputs_are_queued_for_cleanup(
        pool: PgPool,
    ) {
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('export-lease-test', 'hash', 'Export Lease', 'SUPER_ADMIN') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert export user");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Export Lease Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert export contest");
        let task_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO submission_export_tasks (contest_id, requested_by, kind) VALUES ($1, $2, 'METADATA_CSV') RETURNING id",
        )
        .bind(contest_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert export task");
        let service = SubmissionService::new(pool.clone());
        let owner = Uuid::new_v4();
        let claimed = service
            .claim_export_task(owner, Duration::from_secs(30))
            .await
            .expect("claim export task")
            .expect("claimed task");
        assert_eq!((claimed.id, claimed.attempts), (task_id, 1));
        assert!(
            !service
                .complete_export_task(
                    task_id,
                    Uuid::new_v4(),
                    "sources",
                    "exports/stale.csv",
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                )
                .await
                .expect("reject stale completion owner")
        );
        assert!(
            service
                .complete_export_task(
                    task_id,
                    owner,
                    "sources",
                    "exports/valid.csv",
                    time::OffsetDateTime::now_utc() + time::Duration::hours(1),
                )
                .await
                .expect("complete export task")
        );

        sqlx::query(
            "UPDATE submission_export_tasks SET status='SUCCEEDED', output_bucket='sources', output_object_key='exports/expired.csv', expires_at=now()-interval '1 second' WHERE id=$1",
        )
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("expire export output");
        assert_eq!(service.expire_export_tasks(10).await.expect("expire exports"), 1);
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM submission_export_tasks WHERE id=$1",
        )
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("load expired export task");
        assert_eq!(status, "EXPIRED");
        let cleanup = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM object_storage_cleanup_tasks WHERE bucket='sources' AND object_key='exports/expired.csv' AND reason='EXPORT_EXPIRED'",
        )
        .fetch_one(&pool)
        .await
        .expect("load export cleanup task");
        assert_eq!(cleanup, 1);
    }
}

impl ExportTaskKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MetadataCsv => "METADATA_CSV",
            Self::SourcesZip => "SOURCES_ZIP",
        }
    }
}

impl FromStr for ExportTaskKind {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "METADATA_CSV" => Ok(Self::MetadataCsv),
            "SOURCES_ZIP" => Ok(Self::SourcesZip),
            _ => Err("unsupported export task kind"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExportTaskRunnerConfig {
    pub poll_interval: Duration,
    pub lease: Duration,
    pub retry_base: Duration,
    pub output_ttl: Duration,
}

pub struct ExportTaskRunner {
    service: SubmissionService,
    storage: ObjectStorageHandle,
    config: ExportTaskRunnerConfig,
    worker_id: Uuid,
}

impl ExportTaskRunner {
    #[must_use]
    pub fn new(
        database: sqlx::PgPool,
        storage: ObjectStorageHandle,
        config: ExportTaskRunnerConfig,
    ) -> Self {
        Self {
            service: SubmissionService::new(database),
            storage,
            config,
            worker_id: Uuid::new_v4(),
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        info!(worker_id = %self.worker_id, "submission export runner started");
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(error) = self.run_once().await {
                        warn!(?error, "submission export runner iteration failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        info!(worker_id = %self.worker_id, "submission export runner stopped");
    }

    pub async fn run_once(&self) -> Result<bool, AppError> {
        let expired = self
            .service
            .expire_export_tasks(100)
            .await
            .map_err(|error| AppError::internal("expire submission export tasks", error))?;
        if expired > 0 {
            info!(expired, "expired submission exports queued for object cleanup");
        }
        let Some(task) = self
            .service
            .claim_export_task(self.worker_id, self.config.lease)
            .await
            .map_err(|error| AppError::internal("claim submission export task", error))?
        else {
            return Ok(expired > 0);
        };
        let kind = match ExportTaskKind::from_str(&task.kind) {
            Ok(kind) => kind,
            Err(message) => {
                self.fail(&task, message).await?;
                return Ok(true);
            }
        };
        let artifact =
            match self.service.generate_export_artifact(task.contest_id, kind, &self.storage).await
            {
                Ok(artifact) => artifact,
                Err(error) => {
                    self.fail(&task, error.code()).await?;
                    return Ok(true);
                }
            };
        let key = format!(
            "exports/contests/{}/task-{}-{}.{}",
            task.contest_id,
            task.id,
            Uuid::new_v4(),
            artifact.extension
        );
        let bucket = self.storage.source_bucket();
        let upload_result = self
            .storage
            .backend()
            .put_file(bucket, &key, Some(artifact.content_type), &artifact.path)
            .await;
        if let Err(error) = tokio::fs::remove_file(&artifact.path).await {
            warn!(path = %artifact.path.display(), %error, "failed to remove export temporary file");
        }
        if let Err(error) = upload_result {
            self.fail(&task, &error.to_string()).await?;
            return Ok(true);
        }
        let ttl = time::Duration::try_from(self.config.output_ttl)
            .map_err(|error| AppError::internal("convert export output TTL", error))?;
        let expires_at = OffsetDateTime::now_utc() + ttl;
        let completed = self
            .service
            .complete_export_task(task.id, self.worker_id, bucket, &key, expires_at)
            .await
            .map_err(|error| AppError::internal("complete submission export task", error))?;
        if !completed {
            match self.storage.backend().delete(bucket, &key).await {
                Ok(()) => {}
                Err(error) => {
                    defer_failed_cleanup(
                        &self.service.database,
                        bucket,
                        &key,
                        "STALE_EXPORT_LEASE",
                        error.to_string(),
                    )
                    .await;
                }
            }
        }
        Ok(true)
    }

    async fn fail(&self, task: &ClaimedExportTask, message: &str) -> Result<(), AppError> {
        self.service
            .fail_export_task(
                task.id,
                self.worker_id,
                retry_delay(self.config.retry_base, task.attempts),
                message,
            )
            .await
            .map_err(|error| AppError::internal("fail submission export task", error))?;
        Ok(())
    }
}

fn retry_delay(base: Duration, attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(10);
    base.saturating_mul(2_u32.saturating_pow(exponent)).min(Duration::from_secs(3_600))
}

#[derive(Debug, Clone, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExportTaskResponse {
    pub id: i64,
    pub contest_id: i64,
    pub requested_by: i64,
    pub kind: String,
    pub status: String,
    pub output_bucket: Option<String>,
    pub output_object_key: Option<String>,
    pub attempts: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, FromRow)]
pub struct ClaimedExportTask {
    pub id: i64,
    pub contest_id: i64,
    pub requested_by: i64,
    pub kind: String,
    pub attempts: i32,
}

impl SubmissionService {
    pub async fn create_export_task(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        kind: ExportTaskKind,
    ) -> Result<ExportTaskResponse, AppError> {
        if contest_id <= 0 {
            return Err(AppError::validation("contestId", "must be positive"));
        }
        require_admin_access(&self.database, contest_id, actor).await?;
        let row = sqlx::query_as::<_, ExportTaskResponse>(
            "INSERT INTO submission_export_tasks (contest_id, requested_by, kind)
             VALUES ($1, $2, $3)
             RETURNING id, contest_id, requested_by, kind, status, output_bucket,
                       output_object_key, attempts, created_at, updated_at, expires_at",
        )
        .bind(contest_id)
        .bind(actor.id)
        .bind(kind.as_str())
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("create submission export task", error))?;
        Ok(row)
    }

    pub async fn get_export_task(
        &self,
        contest_id: i64,
        task_id: i64,
        actor: &AuthUser,
    ) -> Result<ExportTaskResponse, AppError> {
        if contest_id <= 0 || task_id <= 0 {
            return Err(AppError::validation("id", "must be positive"));
        }
        require_admin_access(&self.database, contest_id, actor).await?;
        sqlx::query_as::<_, ExportTaskResponse>(
            "SELECT id, contest_id, requested_by, kind, status, output_bucket,
                    output_object_key, attempts, created_at, updated_at, expires_at
             FROM submission_export_tasks WHERE contest_id = $1 AND id = $2",
        )
        .bind(contest_id)
        .bind(task_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load submission export task", error))?
        .ok_or_else(|| AppError::not_found("EXPORT_TASK_NOT_FOUND", "Export task was not found"))
    }

    pub async fn claim_export_task(
        &self,
        worker_id: Uuid,
        lease: Duration,
    ) -> Result<Option<ClaimedExportTask>, sqlx::Error> {
        sqlx::query_as(
            "WITH candidate AS (
                 SELECT id FROM submission_export_tasks
                 WHERE (status IN ('QUEUED', 'FAILED') AND available_at <= now())
                    OR (status = 'PROCESSING' AND lease_until < now())
                 ORDER BY available_at, id FOR UPDATE SKIP LOCKED LIMIT 1
             )
             UPDATE submission_export_tasks task
             SET status = 'PROCESSING', attempts = task.attempts + 1,
                 lease_owner = $1,
                 lease_until = now() + $2 * interval '1 millisecond',
                 updated_at = now()
             FROM candidate
             WHERE task.id = candidate.id
             RETURNING task.id, task.contest_id, task.requested_by, task.kind, task.attempts",
        )
        .bind(worker_id)
        .bind(i64::try_from(lease.as_millis()).unwrap_or(i64::MAX))
        .fetch_optional(&self.database)
        .await
    }

    pub async fn complete_export_task(
        &self,
        task_id: i64,
        worker_id: Uuid,
        bucket: &str,
        object_key: &str,
        expires_at: OffsetDateTime,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE submission_export_tasks
             SET status = 'SUCCEEDED', output_bucket = $3, output_object_key = $4,
                 expires_at = $5, lease_owner = NULL, lease_until = NULL, updated_at = now()
             WHERE id = $1 AND status = 'PROCESSING' AND lease_owner = $2",
        )
        .bind(task_id)
        .bind(worker_id)
        .bind(bucket)
        .bind(object_key)
        .bind(expires_at)
        .execute(&self.database)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn fail_export_task(
        &self,
        task_id: i64,
        worker_id: Uuid,
        retry_after: Duration,
        message: &str,
    ) -> Result<bool, sqlx::Error> {
        let safe_message: String = message.chars().take(1_000).collect();
        let result = sqlx::query(
            "UPDATE submission_export_tasks
             SET status = 'FAILED', available_at = now() + $3 * interval '1 millisecond',
                 lease_owner = NULL, lease_until = NULL, last_error = $4, updated_at = now()
             WHERE id = $1 AND status = 'PROCESSING' AND lease_owner = $2",
        )
        .bind(task_id)
        .bind(worker_id)
        .bind(i64::try_from(retry_after.as_millis()).unwrap_or(i64::MAX))
        .bind(safe_message)
        .execute(&self.database)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn expire_export_tasks(&self, batch_size: i64) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "WITH candidates AS (
                 SELECT id
                 FROM submission_export_tasks
                 WHERE status = 'SUCCEEDED' AND expires_at <= now()
                 ORDER BY expires_at, id
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             ), expired AS (
                 UPDATE submission_export_tasks task
                 SET status = 'EXPIRED', updated_at = now()
                 FROM candidates
                 WHERE task.id = candidates.id
                 RETURNING task.output_bucket, task.output_object_key
             ), queued AS (
                 INSERT INTO object_storage_cleanup_tasks (bucket, object_key, reason)
                 SELECT output_bucket, output_object_key, 'EXPORT_EXPIRED'
                 FROM expired
                 WHERE output_bucket IS NOT NULL AND output_object_key IS NOT NULL
                 ON CONFLICT (bucket, object_key) DO NOTHING
                 RETURNING id
             )
             SELECT count(*) FROM expired",
        )
        .bind(batch_size)
        .fetch_one(&self.database)
        .await
    }
}
