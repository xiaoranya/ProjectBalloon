use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use sqlx::PgPool;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::object_storage::ObjectStorageHandle;

use super::{
    ClaimedCleanup, IntegrityScanError, ORPHAN_SCAN_GRACE, ORPHAN_SCAN_INTERVAL,
    ObjectStorageCleanupConfig, ObjectStorageCleanupRunner, SOURCE_PURGE_INTERVAL,
    referenced_object_keys, scan_object_integrity,
};

impl ObjectStorageCleanupRunner {
    #[must_use]
    pub fn new(
        database: PgPool,
        storage: ObjectStorageHandle,
        config: ObjectStorageCleanupConfig,
    ) -> Self {
        Self { database, storage, config, instance_id: Uuid::new_v4() }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_orphan_scan = SystemTime::now();
        let mut last_source_purge = SystemTime::now();
        info!(instance_id = %self.instance_id, "object-storage cleanup runner started");
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if last_orphan_scan.elapsed().is_ok_and(|elapsed| elapsed >= ORPHAN_SCAN_INTERVAL) {
                        match self.scan_orphans_once().await {
                            Ok(count) if count > 0 => info!(count, "object-storage orphan scan queued cleanup tasks"),
                            Ok(_) => {}
                            Err(scan_error) => warn!(%scan_error, "object-storage orphan scan failed"),
                        }
                        last_orphan_scan = SystemTime::now();
                    }
                    if last_source_purge.elapsed().is_ok_and(|elapsed| elapsed >= SOURCE_PURGE_INTERVAL) {
                        match self.purge_expired_practice_sources_once().await {
                            Ok(count) if count > 0 => info!(count, "expired practice sources purged"),
                            Ok(_) => {}
                            Err(purge_error) => warn!(%purge_error, "expired practice source purge failed"),
                        }
                        last_source_purge = SystemTime::now();
                    }
                    match self.run_once().await {
                        Ok(count) if count > 0 => info!(count, "object-storage cleanup batch processed"),
                        Ok(_) => {}
                        Err(cleanup_error) => error!(%cleanup_error, "object-storage cleanup batch failed"),
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        info!(instance_id = %self.instance_id, "object-storage cleanup runner stopped");
    }

    pub async fn scan_orphans_once(&self) -> Result<usize, IntegrityScanError> {
        let mut total = 0;
        let mut scanned = HashSet::new();
        for bucket in [self.storage.problem_bucket(), self.storage.source_bucket()] {
            if !scanned.insert(bucket) {
                continue;
            }
            let references = referenced_object_keys(&self.database, &self.storage, bucket).await?;
            let prefixes: &[&str] = if self.storage.problem_bucket() == self.storage.source_bucket()
            {
                &["problems/", "submissions/", "practice-submissions/", "prints/", "exports/"]
            } else if bucket == self.storage.problem_bucket() {
                &["problems/"]
            } else {
                &["submissions/", "practice-submissions/", "prints/", "exports/"]
            };
            let report = scan_object_integrity(
                &self.database,
                &self.storage,
                bucket,
                prefixes,
                &references,
                ORPHAN_SCAN_GRACE,
            )
            .await?;
            total += report.queued_orphans;
            if report.missing_references > 0 {
                warn!(
                    bucket,
                    missing_references = report.missing_references,
                    "object-storage scan found missing referenced objects"
                );
            }
        }
        Ok(total)
    }

    pub async fn run_once(&self) -> Result<usize, sqlx::Error> {
        let claimed = self.claim().await?;
        let count = claimed.len();
        for task in claimed {
            match self.storage.backend().delete(&task.bucket, &task.object_key).await {
                Ok(()) => self.mark_deleted(task.id).await?,
                Err(storage_error) => {
                    warn!(
                        cleanup_id = task.id,
                        bucket = %task.bucket,
                        object_key = %task.object_key,
                        %storage_error,
                        "object-storage cleanup failed; scheduling retry"
                    );
                    self.mark_failed(task.id, task.attempts, &storage_error.to_string()).await?;
                }
            }
        }
        Ok(count)
    }

    /// Deletes source objects for completed practice submissions after the
    /// administrator-configured retention window. A failed object deletion is
    /// left untouched so the next run can retry it safely.
    pub async fn purge_expired_practice_sources_once(&self) -> Result<usize, sqlx::Error> {
        let days = sqlx::query_scalar::<_, i32>(
            "SELECT source_retention_days FROM practice_platform_settings WHERE singleton=true",
        )
        .fetch_one(&self.database)
        .await?;
        let candidates = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, source_object_key FROM submissions WHERE submission_scope='PRACTICE' AND source_deleted_at IS NULL AND submitted_at < now() - make_interval(days => $1) AND status NOT IN ('PENDING','JUDGING') ORDER BY submitted_at,id LIMIT $2",
        )
        .bind(days)
        .bind(self.config.batch_size)
        .fetch_all(&self.database)
        .await?;
        let mut deleted_ids = Vec::with_capacity(candidates.len());
        for (id, key) in candidates {
            match self.storage.backend().delete(self.storage.source_bucket(), &key).await {
                Ok(()) => deleted_ids.push(id),
                Err(error) => warn!(submission_id = id, %error, "practice source deletion failed"),
            }
        }
        if deleted_ids.is_empty() {
            return Ok(0);
        }
        let purged = sqlx::query(
            "UPDATE submissions SET source_deleted_at=now() WHERE id=ANY($1) AND submission_scope='PRACTICE' AND source_deleted_at IS NULL AND submitted_at < now() - make_interval(days => $2) AND status NOT IN ('PENDING','JUDGING')",
        )
        .bind(&deleted_ids)
        .bind(days)
        .execute(&self.database)
        .await?;
        Ok(usize::try_from(purged.rows_affected()).unwrap_or(0))
    }

    async fn claim(&self) -> Result<Vec<ClaimedCleanup>, sqlx::Error> {
        let lease_milliseconds = duration_millis(self.config.lease);
        sqlx::query_as(
            r#"
            WITH candidates AS (
                SELECT id
                FROM object_storage_cleanup_tasks
                WHERE (status IN ('PENDING', 'FAILED') AND available_at <= now())
                   OR (status = 'PROCESSING' AND lease_until < now())
                ORDER BY available_at, id
                FOR UPDATE SKIP LOCKED
                LIMIT $1
            )
            UPDATE object_storage_cleanup_tasks AS cleanup
            SET status = 'PROCESSING',
                attempts = cleanup.attempts + 1,
                lease_owner = $2,
                lease_until = now() + $3 * interval '1 millisecond',
                last_error = NULL,
                updated_at = now()
            FROM candidates
            WHERE cleanup.id = candidates.id
            RETURNING cleanup.id, cleanup.bucket, cleanup.object_key, cleanup.attempts
            "#,
        )
        .bind(self.config.batch_size)
        .bind(self.instance_id)
        .bind(lease_milliseconds)
        .fetch_all(&self.database)
        .await
    }

    async fn mark_deleted(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM object_storage_cleanup_tasks WHERE id = $1 AND status = 'PROCESSING' AND lease_owner = $2",
        )
        .bind(id)
        .bind(self.instance_id)
        .execute(&self.database)
        .await
        .map(|_| ())
    }

    async fn mark_failed(&self, id: i64, attempts: i32, message: &str) -> Result<(), sqlx::Error> {
        let delay_milliseconds = duration_millis(retry_delay(self.config.retry_base, attempts));
        let safe_message: String = message.chars().take(1_000).collect();
        sqlx::query(
            r#"
            UPDATE object_storage_cleanup_tasks
            SET status = 'FAILED',
                available_at = now() + $3 * interval '1 millisecond',
                lease_owner = NULL,
                lease_until = NULL,
                last_error = $4,
                updated_at = now()
            WHERE id = $1 AND status = 'PROCESSING' AND lease_owner = $2
            "#,
        )
        .bind(id)
        .bind(self.instance_id)
        .bind(delay_milliseconds)
        .bind(safe_message)
        .execute(&self.database)
        .await
        .map(|_| ())
    }
}

pub(crate) fn retry_delay(base: Duration, attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(10);
    base.saturating_mul(2_u32.saturating_pow(exponent)).min(Duration::from_secs(3_600))
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
