use std::path::PathBuf;

use bytes::Bytes;
use time::OffsetDateTime;
use uuid::Uuid;

mod helpers;
mod service;

#[cfg(test)]
mod tests;

const MAX_SYNC_SOURCE_COUNT: i64 = 10_000;
const MAX_SYNC_SOURCE_BYTES: i64 = 128 * 1024 * 1024;
// The synchronous metadata CSV is built entirely in memory, so cap the row
// count the same way the synchronous source export is capped. Larger exports
// should use the async export-task path, which streams to a temp file.
const MAX_SYNC_METADATA_ROWS: i64 = 10_000;
const MAX_ASYNC_SOURCE_BYTES: i64 = 2 * 1024 * 1024 * 1024;

#[derive(sqlx::FromRow)]
struct ExportRow {
    id: i64,
    contest_id: i64,
    problem_id: i64,
    problem_alias: String,
    team_id: i64,
    team_name: String,
    language: String,
    source_size_bytes: i32,
    source_sha256: Option<String>,
    status: String,
    verdict: Option<String>,
    total_time_ms: Option<i32>,
    peak_memory_kb: Option<i32>,
    submitted_at: OffsetDateTime,
    judged_at: Option<OffsetDateTime>,
    active_judgement_id: Option<Uuid>,
    source_object_key: String,
}

struct SourceFile {
    path: String,
    bytes: Bytes,
    sha256: String,
}

struct SourceManifestEntry {
    path: String,
    sha256: String,
}

pub(crate) struct ExportArtifact {
    pub path: PathBuf,
    pub extension: &'static str,
    pub content_type: &'static str,
}
