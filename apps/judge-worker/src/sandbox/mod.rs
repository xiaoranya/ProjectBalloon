use std::time::Duration;

use bollard::Docker;
use project_balloon_contracts::{JudgeRunResult, JudgeVerdict};
use thiserror::Error;

mod archive;
mod compare;
mod fs;
mod language;
mod metrics;
mod runner;

#[cfg(test)]
mod tests;

const MAX_EXEC_LOG_BYTES: usize = 64 * 1024;
const DOCKER_API_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_TESTDATA_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TESTDATA_FILES: usize = 10_000;
const MAX_TESTDATA_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;

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
}
