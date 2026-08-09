use std::{collections::HashSet, time::Duration};

use sqlx::PgPool;

use crate::object_storage::{ObjectStorageHandle, ObjectStorageObject};

use super::enqueue_cleanup_batch;

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
