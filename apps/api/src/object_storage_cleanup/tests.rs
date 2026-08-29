use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use bytes::Bytes;
use sqlx::PgPool;

use crate::object_storage::{
    ObjectStorage, ObjectStorageError, ObjectStorageHandle, ObjectStorageObject,
};
use crate::object_storage_cleanup::{
    ObjectStorageCleanupConfig, ObjectStorageCleanupRunner, enqueue_cleanup, is_orphan_candidate,
    missing_object_keys, reconcile_missing_references, retry_delay,
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
        &ObjectStorageObject { key: "operator/keep".to_owned(), last_modified: old.last_modified },
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
        self.deleted.lock().expect("deleted object lock").push((bucket.to_owned(), key.to_owned()));
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
