use std::{collections::HashSet, path::Path, time::Duration};

use bollard::Docker;
use project_balloon_contracts::{JudgeRunResult, JudgeVerdict};
use thiserror::Error;
use uuid::Uuid;

mod archive;
#[cfg(target_os = "linux")]
pub mod bwrap;
#[cfg(target_os = "linux")]
pub mod cgroup;
mod compare;
pub(crate) mod fs;
pub mod gc;
mod language;
mod metrics;
mod runner;

#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
pub use bwrap::{BubblewrapSandbox, BubblewrapSandboxConfig};
pub use gc::{OrphanSweep, SandboxJanitor, run_orphan_sweeps};

/// The sandbox execution backend a worker instance uses. `docker` is the
/// Docker-compatible (rootless Podman + gVisor in production) container
/// backend; `bwrap` is the non-container bubblewrap backend that runs
/// submissions directly against a Linux host with namespaces and cgroup v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackend {
    Docker,
    #[cfg(target_os = "linux")]
    Bubblewrap,
}

impl SandboxBackend {
    /// Parses the `SANDBOX_BACKEND` configuration value.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "docker" => Ok(Self::Docker),
            #[cfg(target_os = "linux")]
            "bwrap" | "bubblewrap" => Ok(Self::Bubblewrap),
            other => {
                Err(format!("unknown sandbox backend {other:?} (expected \"docker\" or \"bwrap\")"))
            }
        }
    }
}

/// The concrete sandbox assembled at startup, dispatching [`SandboxJudge`]
/// and [`SandboxJanitor`] over the configured backend. Exists so `main` can
/// hold one type across backends and the generic engine stays monomorphized.
#[derive(Clone)]
pub enum JudgeSandbox {
    Docker(Box<DockerSandbox>),
    #[cfg(target_os = "linux")]
    Bubblewrap(BubblewrapSandbox),
}

impl JudgeSandbox {
    /// Human-readable runtime label reported through worker heartbeats. The
    /// Docker backend keeps the operator-configurable `XCPC_SANDBOX_RUNTIME`
    /// override; the bubblewrap backend has no OCI runtime to configure.
    pub fn runtime_label(&self, docker_runtime: Option<&str>) -> String {
        match self {
            Self::Docker(_) => docker_runtime.unwrap_or("docker-default").to_owned(),
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(_) => "bwrap-namespaces".to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl crate::worker::SandboxJudge for JudgeSandbox {
    async fn preflight(&self) -> Result<(), SandboxError> {
        match self {
            Self::Docker(sandbox) => sandbox.preflight().await,
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(sandbox) => sandbox.preflight().await,
        }
    }

    async fn judge(
        &self,
        task: &project_balloon_contracts::JudgeTask,
        source: &[u8],
        archive: &Path,
        interactor: Option<&[u8]>,
    ) -> Result<SandboxJudgement, SandboxError> {
        match self {
            Self::Docker(sandbox) => sandbox.judge(task, source, archive, interactor).await,
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(sandbox) => sandbox.judge(task, source, archive, interactor).await,
        }
    }
}

#[async_trait::async_trait]
impl SandboxJanitor for JudgeSandbox {
    async fn sweep_orphans(&self, keep: &HashSet<Uuid>) -> Result<OrphanSweep, SandboxError> {
        match self {
            Self::Docker(sandbox) => sandbox.sweep_orphans(keep).await,
            #[cfg(target_os = "linux")]
            Self::Bubblewrap(sandbox) => sandbox.sweep_orphans(keep).await,
        }
    }
}

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
