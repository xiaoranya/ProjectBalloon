use sqlx::{PgPool, Postgres, Transaction};
use tracing::{error, warn};

use crate::object_storage::ObjectStorageHandle;

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

pub(crate) async fn enqueue_cleanup_batch(
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
