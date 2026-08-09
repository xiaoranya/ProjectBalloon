use std::time::Duration;

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::object_storage::ObjectStorageHandle;

mod integrity;
mod queue;
mod runner;

#[cfg(test)]
mod tests;

pub use integrity::{
    ObjectStorageIntegrityReport, referenced_object_keys, scan_object_integrity,
    scan_orphaned_objects,
};
#[cfg(test)]
pub(crate) use integrity::{
    is_orphan_candidate, missing_object_keys, reconcile_missing_references,
};
pub(crate) use queue::enqueue_cleanup_batch;
pub use queue::{
    attempt_queued_cleanup, defer_failed_cleanup, enqueue_cleanup, enqueue_cleanup_transaction,
};
#[cfg(test)]
pub(crate) use runner::retry_delay;

#[derive(Clone, Copy, Debug)]
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
