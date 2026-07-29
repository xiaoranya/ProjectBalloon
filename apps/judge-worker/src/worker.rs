use project_balloon_contracts::{
    JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeTask, JudgeVerdict,
};
use time::OffsetDateTime;

use crate::{
    artifacts::{ArtifactError, ArtifactManager},
    rabbit::{JudgeTaskHandler, TaskFailure},
    sandbox::{DockerSandbox, SandboxError, SandboxJudgement},
};

pub struct JudgeEngine {
    worker_id: String,
    artifacts: ArtifactManager,
    sandbox: DockerSandbox,
}

const MAX_TASK_RETRIES: u32 = 8;

impl JudgeEngine {
    #[must_use]
    pub fn new(worker_id: String, artifacts: ArtifactManager, sandbox: DockerSandbox) -> Self {
        Self { worker_id, artifacts, sandbox }
    }

    pub async fn preflight(&self) -> Result<(), TaskFailure> {
        self.artifacts.preflight().await.map_err(|error| TaskFailure::retry(error.to_string()))?;
        self.sandbox.preflight().await.map_err(|error| TaskFailure::retry(error.to_string()))?;
        Ok(())
    }

    fn system_error_result(
        &self,
        task: &JudgeTask,
        started_at: OffsetDateTime,
        reason: String,
    ) -> JudgeResult {
        JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: task.judgement_id,
            judgement_id: task.judgement_id,
            submission_id: task.submission_id,
            worker_id: self.worker_id.clone(),
            verdict: JudgeVerdict::SystemError,
            total_time_ms: 0,
            peak_memory_kb: 0,
            compile_log: Some(truncate_utf8(&reason, 64 * 1024)),
            started_at,
            completed_at: OffsetDateTime::now_utc(),
            runs: Vec::new(),
        }
    }

    fn completed_result(
        &self,
        task: &JudgeTask,
        started_at: OffsetDateTime,
        judgement: SandboxJudgement,
    ) -> JudgeResult {
        JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: task.judgement_id,
            judgement_id: task.judgement_id,
            submission_id: task.submission_id,
            worker_id: self.worker_id.clone(),
            verdict: judgement.verdict,
            total_time_ms: judgement.total_time_ms,
            peak_memory_kb: judgement.peak_memory_kb,
            compile_log: judgement.compile_log,
            started_at,
            completed_at: OffsetDateTime::now_utc(),
            runs: judgement.runs,
        }
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    let end = value
        .char_indices()
        .take_while(|(index, character)| index + character.len_utf8() <= max_bytes)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    value[..end].to_owned()
}

#[async_trait::async_trait]
impl JudgeTaskHandler for JudgeEngine {
    async fn handle(&self, task: JudgeTask, retry_count: u32) -> Result<JudgeResult, TaskFailure> {
        let started_at = OffsetDateTime::now_utc();
        let artifacts = match self.artifacts.prepare(&task).await {
            Ok(artifacts) => artifacts,
            Err(error @ (ArtifactError::HashMismatch { .. } | ArtifactError::TooLarge(_))) => {
                return Ok(self.system_error_result(&task, started_at, error.to_string()));
            }
            Err(error) if retry_count >= MAX_TASK_RETRIES => {
                return Ok(self.system_error_result(&task, started_at, error.to_string()));
            }
            Err(error) => return Err(TaskFailure::retry(error.to_string())),
        };
        match self.sandbox.judge(&task, &artifacts.source, &artifacts.testdata_archive).await {
            Ok(judgement) => Ok(self.completed_result(&task, started_at, judgement)),
            Err(error @ SandboxError::InvalidTestdata(_)) => {
                Ok(self.system_error_result(&task, started_at, error.to_string()))
            }
            Err(error) if retry_count >= MAX_TASK_RETRIES => {
                Ok(self.system_error_result(&task, started_at, error.to_string()))
            }
            Err(error) => Err(TaskFailure::retry(error.to_string())),
        }
    }
}
