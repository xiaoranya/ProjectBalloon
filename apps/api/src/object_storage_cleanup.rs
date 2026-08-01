use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use sqlx::{FromRow, PgPool, Postgres, Transaction};
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::object_storage::{ObjectStorageHandle, ObjectStorageObject};

#[derive(Debug, Clone, Copy)]
pub struct ObjectStorageCleanupConfig {
    pub poll_interval: Duration,
    pub lease: Duration,
    pub retry_base: Duration,
    pub batch_size: i64,
}

const ORPHAN_SCAN_INTERVAL: Duration = Duration::from_secs(3_600);
const ORPHAN_SCAN_GRACE: Duration = Duration::from_secs(900);
const SOURCE_PURGE_INTERVAL: Duration = Duration::from_secs(3_600);

#[derive(Clone)]
pub struct ObjectStorageCleanupRunner {
    database: PgPool,
    storage: ObjectStorageHandle,
    config: ObjectStorageCleanupConfig,
    instance_id: Uuid,
}

#[derive(Debug, FromRow)]
struct ClaimedCleanup {
    id: i64,
    bucket: String,
    object_key: String,
    attempts: i32,
}

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

    pub async fn scan_orphans_once(&self) -> Result<usize, sqlx::Error> {
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

pub async fn enqueue_cleanup(
    database: &PgPool,
    bucket: &str,
    object_key: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO object_storage_cleanup_tasks (bucket, object_key, reason)
        VALUES ($1, $2, $3)
        ON CONFLICT (bucket, object_key) DO NOTHING
        "#,
    )
    .bind(bucket)
    .bind(object_key)
    .bind(reason)
    .execute(database)
    .await
    .map(|_| ())
}

async fn enqueue_cleanup_batch(
    database: &PgPool,
    bucket: &str,
    object_keys: &[String],
    reason: &str,
) -> Result<(), sqlx::Error> {
    if object_keys.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO object_storage_cleanup_tasks (bucket, object_key, reason)
        SELECT $1, object_key, $3
        FROM unnest($2::text[]) AS object_key
        ON CONFLICT (bucket, object_key) DO NOTHING
        "#,
    )
    .bind(bucket)
    .bind(object_keys)
    .bind(reason)
    .execute(database)
    .await
    .map(|_| ())
}

pub async fn enqueue_cleanup_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    bucket: &str,
    object_key: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO object_storage_cleanup_tasks (bucket, object_key, reason)
        VALUES ($1, $2, $3)
        ON CONFLICT (bucket, object_key) DO NOTHING
        "#,
    )
    .bind(bucket)
    .bind(object_key)
    .bind(reason)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
}

pub async fn attempt_queued_cleanup(
    database: &PgPool,
    storage: &ObjectStorageHandle,
    bucket: &str,
    object_key: &str,
) {
    match storage.backend().delete(bucket, object_key).await {
        Ok(()) => {
            if let Err(database_error) = sqlx::query(
                "DELETE FROM object_storage_cleanup_tasks WHERE bucket = $1 AND object_key = $2",
            )
            .bind(bucket)
            .bind(object_key)
            .execute(database)
            .await
            {
                error!(
                    bucket,
                    object_key,
                    %database_error,
                    "object was deleted but cleanup task could not be removed; idempotent retry remains"
                );
            }
        }
        Err(storage_error) => warn!(
            bucket,
            object_key,
            %storage_error,
            "immediate object cleanup failed; persisted task remains for background retry"
        ),
    }
}

pub async fn defer_failed_cleanup(
    database: &PgPool,
    bucket: &str,
    object_key: &str,
    reason: &str,
    storage_error: String,
) {
    match enqueue_cleanup(database, bucket, object_key, reason).await {
        Ok(()) => warn!(
            bucket,
            object_key, reason, storage_error, "object cleanup persisted for background retry"
        ),
        Err(database_error) => error!(
            bucket,
            object_key,
            reason,
            storage_error,
            %database_error,
            "object cleanup failed and could not be persisted"
        ),
    }
}

/// Finds unreferenced objects in a bucket and persists them as idempotent
/// cleanup tasks. Only the explicitly supplied key prefixes are considered;
/// this prevents operator-managed objects from being touched by a scan.
pub async fn scan_orphaned_objects(
    database: &PgPool,
    storage: &ObjectStorageHandle,
    bucket: &str,
    prefixes: &[&str],
    referenced_keys: &HashSet<String>,
    grace: Duration,
) -> Result<usize, sqlx::Error> {
    scan_object_integrity(database, storage, bucket, prefixes, referenced_keys, grace)
        .await
        .map(|report| report.queued_orphans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectStorageIntegrityReport {
    pub queued_orphans: usize,
    pub missing_references: usize,
}

/// Reconciles both directions of the database/object-storage relationship.
/// Unreferenced owned objects are queued for deletion, while references whose
/// objects are absent are retained as operational findings until a later scan
/// observes recovery or removal of the database reference.
pub async fn scan_object_integrity(
    database: &PgPool,
    storage: &ObjectStorageHandle,
    bucket: &str,
    prefixes: &[&str],
    referenced_keys: &HashSet<String>,
    grace: Duration,
) -> Result<ObjectStorageIntegrityReport, sqlx::Error> {
    let mut token = None;
    let mut missing = referenced_keys.clone();
    let mut queued_orphans = 0;
    loop {
        let page = storage
            .backend()
            .list_objects(bucket, token.as_deref())
            .await
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let mut orphan_keys = Vec::new();
        for object in page.objects {
            missing.remove(&object.key);
            if is_orphan_candidate(&object, prefixes, referenced_keys, grace) {
                orphan_keys.push(object.key);
            }
        }
        enqueue_cleanup_batch(database, bucket, &orphan_keys, "ORPHAN_SCAN").await?;
        queued_orphans += orphan_keys.len();
        token = page.next_continuation_token;
        if token.is_none() {
            break;
        }
    }
    let mut missing: Vec<String> = missing.into_iter().collect();
    missing.sort_unstable();
    reconcile_missing_references(database, bucket, &missing).await?;
    Ok(ObjectStorageIntegrityReport { queued_orphans, missing_references: missing.len() })
}

#[cfg(test)]
fn missing_object_keys(
    referenced_keys: &HashSet<String>,
    listed_keys: &HashSet<&str>,
) -> Vec<String> {
    let mut missing: Vec<String> =
        referenced_keys.iter().filter(|key| !listed_keys.contains(key.as_str())).cloned().collect();
    missing.sort_unstable();
    missing
}

async fn reconcile_missing_references(
    database: &PgPool,
    bucket: &str,
    missing: &[String],
) -> Result<(), sqlx::Error> {
    let mut transaction = database.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO object_storage_integrity_findings (bucket, object_key)
        SELECT $1, object_key
        FROM unnest($2::text[]) AS object_key
        ON CONFLICT (bucket, object_key) DO UPDATE
        SET last_detected_at = now(), resolved_at = NULL
        "#,
    )
    .bind(bucket)
    .bind(missing)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        UPDATE object_storage_integrity_findings
        SET resolved_at = now()
        WHERE bucket = $1
          AND resolved_at IS NULL
          AND NOT (object_key = ANY($2::text[]))
        "#,
    )
    .bind(bucket)
    .bind(missing)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

fn is_orphan_candidate(
    object: &ObjectStorageObject,
    prefixes: &[&str],
    referenced_keys: &HashSet<String>,
    grace: Duration,
) -> bool {
    prefixes.iter().any(|prefix| object.key.starts_with(prefix))
        && !referenced_keys.contains(&object.key)
        && object
            .last_modified
            .is_some_and(|modified| modified.elapsed().is_ok_and(|age| age >= grace))
}

/// Loads the authoritative references for one configured bucket. The query
/// deliberately reads every durable object reference, including historical
/// test-data versions and completed submissions, so a scan cannot remove an
/// object that is still needed for export or rejudging.
pub async fn referenced_object_keys(
    database: &PgPool,
    storage: &ObjectStorageHandle,
    bucket: &str,
) -> Result<std::collections::HashSet<String>, sqlx::Error> {
    let keys: Vec<String> =
        if bucket == storage.problem_bucket() && bucket == storage.source_bucket() {
            sqlx::query_scalar(
                "SELECT object_key FROM problem_attachments
             UNION SELECT object_key FROM problem_testdata_versions
             UNION SELECT testdata_object_key FROM problems
             WHERE testdata_object_key IS NOT NULL
             UNION SELECT interactor_object_key FROM problems
             WHERE interactor_object_key IS NOT NULL
             UNION SELECT source_object_key FROM submissions WHERE source_deleted_at IS NULL
             UNION SELECT pdf_object_key FROM print_requests
             WHERE pdf_bucket = $1 AND pdf_object_key IS NOT NULL
             UNION SELECT output_object_key FROM submission_export_tasks
             WHERE output_bucket = $1 AND output_object_key IS NOT NULL
               AND status = 'SUCCEEDED' AND expires_at > now()",
            )
            .bind(bucket)
            .fetch_all(database)
            .await?
        } else if bucket == storage.problem_bucket() {
            sqlx::query_scalar(
                "SELECT object_key FROM problem_attachments
             UNION SELECT object_key FROM problem_testdata_versions
             UNION SELECT testdata_object_key FROM problems
             WHERE testdata_object_key IS NOT NULL
             UNION SELECT interactor_object_key FROM problems
             WHERE interactor_object_key IS NOT NULL
             UNION SELECT output_object_key FROM submission_export_tasks
             WHERE output_bucket = $1 AND output_object_key IS NOT NULL
               AND status = 'SUCCEEDED' AND expires_at > now()",
            )
            .fetch_all(database)
            .await?
        } else if bucket == storage.source_bucket() {
            sqlx::query_scalar(
                "SELECT source_object_key FROM submissions WHERE source_deleted_at IS NULL
             UNION SELECT pdf_object_key FROM print_requests
             WHERE pdf_bucket = $1 AND pdf_object_key IS NOT NULL
             UNION SELECT output_object_key FROM submission_export_tasks
             WHERE output_bucket = $1 AND output_object_key IS NOT NULL
               AND status = 'SUCCEEDED' AND expires_at > now()",
            )
            .bind(bucket)
            .fetch_all(database)
            .await?
        } else {
            return Ok(std::collections::HashSet::new());
        };
    Ok(keys.into_iter().collect())
}

fn retry_delay(base: Duration, attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(10);
    base.saturating_mul(2_u32.saturating_pow(exponent)).min(Duration::from_secs(3_600))
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
        time::{Duration, SystemTime},
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use sqlx::PgPool;

    use super::{
        ObjectStorageCleanupConfig, ObjectStorageCleanupRunner, enqueue_cleanup,
        is_orphan_candidate, missing_object_keys, reconcile_missing_references, retry_delay,
    };
    use crate::object_storage::{
        ObjectStorage, ObjectStorageError, ObjectStorageHandle, ObjectStorageObject,
    };

    #[test]
    fn orphan_scan_requires_old_listed_object_and_owned_prefix() {
        let old = ObjectStorageObject {
            key: "problems/1/a".to_owned(),
            last_modified: Some(SystemTime::now() - Duration::from_secs(3_600)),
        };
        let mut references = HashSet::new();
        assert!(is_orphan_candidate(&old, &["problems/"], &references, Duration::from_secs(900)));
        references.insert(old.key.clone());
        assert!(!is_orphan_candidate(&old, &["problems/"], &references, Duration::from_secs(900)));
        assert!(!is_orphan_candidate(
            &ObjectStorageObject {
                key: "operator/keep".to_owned(),
                last_modified: old.last_modified,
            },
            &["problems/"],
            &HashSet::new(),
            Duration::from_secs(900),
        ));
        assert!(!is_orphan_candidate(
            &ObjectStorageObject { key: old.key, last_modified: None },
            &["problems/"],
            &HashSet::new(),
            Duration::from_secs(900),
        ));
    }

    #[test]
    fn missing_reference_detection_is_complete_and_deterministic() {
        let references = HashSet::from([
            "problems/2/testdata.zip".to_owned(),
            "problems/1/statement.pdf".to_owned(),
        ]);
        let listed = HashSet::from(["problems/2/testdata.zip", "operator/unmanaged"]);

        assert_eq!(
            missing_object_keys(&references, &listed),
            vec!["problems/1/statement.pdf".to_owned()]
        );
    }

    struct RecoveringStorage {
        fail_delete: Mutex<bool>,
        deleted: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl ObjectStorage for RecoveringStorage {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
            Ok(())
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

        async fn get(&self, _bucket: &str, _key: &str) -> Result<Bytes, ObjectStorageError> {
            Err(ObjectStorageError::Request("not implemented".to_owned()))
        }

        async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
            if *self.fail_delete.lock().expect("delete failure lock") {
                return Err(ObjectStorageError::Request("temporary failure".to_owned()));
            }
            self.deleted
                .lock()
                .expect("deleted object lock")
                .push((bucket.to_owned(), key.to_owned()));
            Ok(())
        }
    }

    #[test]
    fn cleanup_retry_is_exponential_and_capped() {
        assert_eq!(retry_delay(Duration::from_secs(1), 1), Duration::from_secs(1));
        assert_eq!(retry_delay(Duration::from_secs(1), 4), Duration::from_secs(8));
        assert_eq!(retry_delay(Duration::from_secs(10), 20), Duration::from_secs(3_600));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn failed_cleanup_is_durable_idempotent_and_retried(pool: PgPool) {
        enqueue_cleanup(&pool, "test-bucket", "orphans/object.txt", "TEST")
            .await
            .expect("enqueue cleanup");
        enqueue_cleanup(&pool, "test-bucket", "orphans/object.txt", "TEST")
            .await
            .expect("enqueue duplicate cleanup");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
                .fetch_one(&pool)
                .await
                .expect("count cleanup rows"),
            1
        );

        let backend = Arc::new(RecoveringStorage {
            fail_delete: Mutex::new(true),
            deleted: Mutex::new(Vec::new()),
        });
        let runner = ObjectStorageCleanupRunner::new(
            pool.clone(),
            ObjectStorageHandle::new(backend.clone(), "test-bucket".to_owned()),
            ObjectStorageCleanupConfig {
                poll_interval: Duration::from_secs(1),
                lease: Duration::from_secs(30),
                retry_base: Duration::from_millis(1),
                batch_size: 10,
            },
        );
        assert_eq!(runner.run_once().await.expect("first cleanup attempt"), 1);
        let failed = sqlx::query_as::<_, (String, i32, Option<String>)>(
            "SELECT status, attempts, last_error FROM object_storage_cleanup_tasks",
        )
        .fetch_one(&pool)
        .await
        .expect("load failed cleanup");
        assert_eq!(failed.0, "FAILED");
        assert_eq!(failed.1, 1);
        assert!(failed.2.is_some_and(|message| message.contains("temporary failure")));

        *backend.fail_delete.lock().expect("delete failure lock") = false;
        sqlx::query("UPDATE object_storage_cleanup_tasks SET available_at = now()")
            .execute(&pool)
            .await
            .expect("make retry available");
        assert_eq!(runner.run_once().await.expect("successful retry"), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
                .fetch_one(&pool)
                .await
                .expect("count completed cleanup rows"),
            0
        );
        assert_eq!(
            *backend.deleted.lock().expect("deleted object lock"),
            vec![("test-bucket".to_owned(), "orphans/object.txt".to_owned())]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn missing_reference_findings_reopen_and_resolve(pool: PgPool) {
        let key = "problems/1/missing.zip".to_owned();
        reconcile_missing_references(&pool, "problem-bucket", std::slice::from_ref(&key))
            .await
            .expect("missing reference is recorded");
        reconcile_missing_references(&pool, "problem-bucket", std::slice::from_ref(&key))
            .await
            .expect("repeated scan is idempotent");

        let finding = sqlx::query_as::<_, (i64, bool)>(
            "SELECT count(*), bool_and(resolved_at IS NULL) FROM object_storage_integrity_findings",
        )
        .fetch_one(&pool)
        .await
        .expect("finding can be loaded");
        assert_eq!(finding, (1, true));

        reconcile_missing_references(&pool, "problem-bucket", &[])
            .await
            .expect("restored reference resolves finding");
        let resolved: bool = sqlx::query_scalar(
            "SELECT resolved_at IS NOT NULL FROM object_storage_integrity_findings WHERE bucket = $1 AND object_key = $2",
        )
        .bind("problem-bucket")
        .bind(key)
        .fetch_one(&pool)
        .await
        .expect("resolved finding can be loaded");
        assert!(resolved);
    }
}
