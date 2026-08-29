use std::{collections::HashSet, time::Duration};

use sqlx::PgPool;

use crate::object_storage::{ObjectStorageError, ObjectStorageHandle, ObjectStorageObject};

use super::enqueue_cleanup_batch;

/// Distinguishes the two failure domains of an integrity scan so that a
/// PostgreSQL outage is not reported as an object-storage outage and
/// vice versa.
#[derive(Debug, thiserror::Error)]
pub enum IntegrityScanError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] ObjectStorageError),
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
) -> Result<ObjectStorageIntegrityReport, IntegrityScanError> {
    let mut token = None;
    let mut missing: HashSet<&str> = referenced_keys.iter().map(String::as_str).collect();
    let mut queued_orphans = 0;
    loop {
        let page = storage.backend().list_objects(bucket, token.as_deref()).await?;
        let mut orphan_keys = Vec::new();
        for object in page.objects {
            missing.remove(object.key.as_str());
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
    let mut missing: Vec<String> = missing.into_iter().map(str::to_owned).collect();
    missing.sort_unstable();
    reconcile_missing_references(database, bucket, &missing).await?;
    Ok(ObjectStorageIntegrityReport { queued_orphans, missing_references: missing.len() })
}

#[cfg(test)]
pub(crate) fn missing_object_keys(
    referenced_keys: &HashSet<String>,
    listed_keys: &HashSet<&str>,
) -> Vec<String> {
    let mut missing: Vec<String> =
        referenced_keys.iter().filter(|key| !listed_keys.contains(key.as_str())).cloned().collect();
    missing.sort_unstable();
    missing
}

pub(crate) async fn reconcile_missing_references(
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

pub(crate) fn is_orphan_candidate(
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

/// Every object key referenced by the database for one bucket class. The
/// `$2`/`$3` booleans gate the problem-domain and source-domain branches so a
/// single statement covers the problem bucket, the source bucket, and a shared
/// bucket (both flags true) without three copies of the UNION.
const REFERENCED_OBJECT_KEYS_SQL: &str = r#"
    SELECT object_key FROM problem_attachments WHERE $2
    UNION SELECT object_key FROM problem_testdata_versions WHERE $2
    UNION SELECT testdata_object_key FROM problems
    WHERE $2 AND testdata_object_key IS NOT NULL
    UNION SELECT interactor_object_key FROM problems
    WHERE $2 AND interactor_object_key IS NOT NULL
    UNION SELECT source_object_key FROM submissions
    WHERE $3 AND source_deleted_at IS NULL
    UNION SELECT pdf_object_key FROM print_requests
    WHERE $3 AND pdf_bucket = $1 AND pdf_object_key IS NOT NULL
    UNION SELECT output_object_key FROM submission_export_tasks
    WHERE output_bucket = $1 AND output_object_key IS NOT NULL
      AND status = 'SUCCEEDED' AND expires_at > now()
"#;

pub async fn referenced_object_keys(
    database: &PgPool,
    storage: &ObjectStorageHandle,
    bucket: &str,
) -> Result<std::collections::HashSet<String>, IntegrityScanError> {
    let is_problem_bucket = bucket == storage.problem_bucket();
    let is_source_bucket = bucket == storage.source_bucket();
    if !is_problem_bucket && !is_source_bucket {
        return Ok(std::collections::HashSet::new());
    }
    let keys: Vec<String> = sqlx::query_scalar(REFERENCED_OBJECT_KEYS_SQL)
        .bind(bucket)
        .bind(is_problem_bucket)
        .bind(is_source_bucket)
        .fetch_all(database)
        .await?;
    Ok(keys.into_iter().collect())
}
