use std::time::Duration;

use bollard::Docker;
use project_balloon_contracts::{JudgeRunResult, JudgeVerdict};
use thiserror::Error;
use uuid::Uuid;

mod archive;
mod compare;
pub(crate) mod fs;
pub mod gc;
mod language;
mod metrics;
mod runner;

#[cfg(test)]
mod tests;

pub use gc::{OrphanSweep, run_orphan_sweeps};

const MAX_EXEC_LOG_BYTES: usize = 64 * 1024;
const MAX_TESTDATA_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TESTDATA_FILES: usize = 10_000;
const MAX_TESTDATA_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;

/// Name prefix of the per-judgement sandbox containers, shared by the runner
/// and the orphan sweeper.
pub(crate) const JUDGE_CONTAINER_PREFIX: &str = "pb-judge-";

/// Wall-clock allowance the sandbox grants the compile exec inside the
/// container.
pub(crate) const COMPILE_WALL_LIMIT: Duration = Duration::from_secs(30);

/// Multiple of the effective time limit the runner grants each run as wall
/// clock.
pub(crate) const RUN_WALL_MULTIPLIER: u64 = 3;

/// Floor of the per-run wall limit, in milliseconds.
pub(crate) const MIN_RUN_WALL_MS: u64 = 1_000;

/// Docker container name for a judgement, shared by the runner and the
/// orphan sweeper.
pub(crate) fn judgement_container_name(judgement_id: Uuid) -> String {
    format!("{JUDGE_CONTAINER_PREFIX}{judgement_id}")
}

/// Parses a Docker container name (Docker prefixes listed names with `/`)
/// into the judgement it belongs to, if it is one of ours.
pub(crate) fn judgement_id_from_container_name(name: &str) -> Option<Uuid> {
    name.strip_prefix('/').unwrap_or(name).strip_prefix(JUDGE_CONTAINER_PREFIX)?.parse().ok()
}

/// Applies the language multiplier to the task time limit, clamped to at least
/// one millisecond.
pub(crate) fn effective_time_limit(task_time_limit_ms: i32, language_multiplier: f64) -> i32 {
    (f64::from(task_time_limit_ms) * language_multiplier).ceil().clamp(1.0, f64::from(i32::MAX))
        as i32
}

/// Wall clock the runner grants a single run: the effective time limit with
/// multiplier headroom, floored at one second.
pub(crate) fn run_wall_limit(effective_time_limit_ms: i32) -> Duration {
    Duration::from_millis(
        u64::try_from(effective_time_limit_ms)
            .unwrap_or(1)
            .saturating_mul(RUN_WALL_MULTIPLIER)
            .max(MIN_RUN_WALL_MS),
    )
}

/// Docker answers 409 when a container with the same name already exists.
pub(crate) fn is_container_name_conflict(error: &bollard::errors::Error) -> bool {
    matches!(error, bollard::errors::Error::DockerResponseServerError { status_code: 409, .. })
}

/// Docker answers 404 when the targeted container no longer exists.
pub(crate) fn is_container_missing(error: &bollard::errors::Error) -> bool {
    matches!(error, bollard::errors::Error::DockerResponseServerError { status_code: 404, .. })
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox API failed: {0}")]
    Api(String),
    #[error("sandbox filesystem failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("test-data archive is invalid: {0}")]
    InvalidTestdata(String),
    #[error("language {0} is not enabled by this Worker slice")]
    UnsupportedLanguage(String),
}

#[derive(Debug)]
pub struct SandboxJudgement {
    pub verdict: JudgeVerdict,
    pub total_time_ms: i32,
    pub peak_memory_kb: i32,
    pub compile_log: Option<String>,
    pub runs: Vec<JudgeRunResult>,
}

#[derive(Clone)]
pub struct DockerSandbox {
    docker: Docker,
    cache_dir: std::path::PathBuf,
    runtime: Option<String>,
    user: String,
    c_image: String,
    cpp_image: String,
    java_image: String,
    python_image: String,
    go_image: String,
    rust_image: String,
    /// Bound for every individual Docker API call (kill, inspect, remove, …).
    docker_api_timeout: Duration,
}

pub struct DockerSandboxConfig {
    pub socket: std::path::PathBuf,
    pub cache_dir: std::path::PathBuf,
    pub runtime: Option<String>,
    pub user: String,
    pub c_image: String,
    pub cpp_image: String,
    pub java_image: String,
    pub python_image: String,
    pub go_image: String,
    pub rust_image: String,
    /// Client timeout in seconds for establishing the Docker connection.
    pub docker_connect_timeout_seconds: u64,
    /// Bound for every individual Docker API call.
    pub docker_api_timeout: Duration,
}
