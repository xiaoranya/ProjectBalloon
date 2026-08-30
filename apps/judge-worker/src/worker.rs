use std::path::Path;

use project_balloon_contracts::{
    JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeTask, JudgeVerdict,
};
use time::OffsetDateTime;

use crate::{
    artifacts::{ArtifactError, ArtifactManager, PreparedArtifacts},
    rabbit::{JudgeTaskHandler, TaskFailure},
    sandbox::{DockerSandbox, SandboxError, SandboxJudgement},
};

/// Artifact acquisition behind [`JudgeEngine`], so the engine's failure
/// mapping can be exercised without an S3 endpoint. Only implemented by
/// [`ArtifactManager`] in production.
#[async_trait::async_trait]
pub trait ArtifactPreparer: Send + Sync {
    async fn preflight(&self) -> Result<(), ArtifactError>;
    async fn prepare(&self, task: &JudgeTask) -> Result<PreparedArtifacts, ArtifactError>;
}

/// Judgement sandbox behind [`JudgeEngine`], so the engine's failure mapping
/// can be exercised without a Docker daemon. Only implemented by
/// [`DockerSandbox`] in production.
#[async_trait::async_trait]
pub trait SandboxJudge: Send + Sync {
    async fn preflight(&self) -> Result<(), SandboxError>;
    async fn judge(
        &self,
        task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        interactor: Option<&[u8]>,
    ) -> Result<SandboxJudgement, SandboxError>;
}

#[async_trait::async_trait]
impl ArtifactPreparer for ArtifactManager {
    async fn preflight(&self) -> Result<(), ArtifactError> {
        ArtifactManager::preflight(self).await
    }

    async fn prepare(&self, task: &JudgeTask) -> Result<PreparedArtifacts, ArtifactError> {
        ArtifactManager::prepare(self, task).await
    }
}

#[async_trait::async_trait]
impl SandboxJudge for DockerSandbox {
    async fn preflight(&self) -> Result<(), SandboxError> {
        DockerSandbox::preflight(self).await
    }

    async fn judge(
        &self,
        task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        interactor: Option<&[u8]>,
    ) -> Result<SandboxJudgement, SandboxError> {
        DockerSandbox::judge(self, task, source, archive, interactor).await
    }
}

pub struct JudgeEngine<A: ArtifactPreparer = ArtifactManager, S: SandboxJudge = DockerSandbox> {
    worker_id: String,
    artifacts: A,
    sandbox: S,
}

const MAX_TASK_RETRIES: u32 = 8;

impl JudgeEngine<ArtifactManager, DockerSandbox> {
    #[must_use]
    pub fn new(worker_id: String, artifacts: ArtifactManager, sandbox: DockerSandbox) -> Self {
        Self { worker_id, artifacts, sandbox }
    }
}

impl<A: ArtifactPreparer, S: SandboxJudge> JudgeEngine<A, S> {
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
impl<A: ArtifactPreparer, S: SandboxJudge> JudgeTaskHandler for JudgeEngine<A, S> {
    async fn handle(&self, task: &JudgeTask, retry_count: u32) -> Result<JudgeResult, TaskFailure> {
        let started_at = OffsetDateTime::now_utc();
        let artifacts = match self.artifacts.prepare(task).await {
            Ok(artifacts) => artifacts,
            Err(error @ (ArtifactError::HashMismatch { .. } | ArtifactError::TooLarge(_))) => {
                return Ok(self.system_error_result(task, started_at, error.to_string()));
            }
            Err(error) if retry_budget_exhausted(retry_count) => {
                return Ok(self.system_error_result(task, started_at, error.to_string()));
            }
            Err(error) => return Err(TaskFailure::retry(error.to_string())),
        };
        match self
            .sandbox
            .judge(
                task,
                &artifacts.source,
                &artifacts.testdata_archive,
                artifacts.interactor.as_deref(),
            )
            .await
        {
            Ok(judgement) => Ok(self.completed_result(task, started_at, judgement)),
            Err(error @ SandboxError::InvalidTestdata(_)) => {
                Ok(self.system_error_result(task, started_at, error.to_string()))
            }
            Err(error) if retry_budget_exhausted(retry_count) => {
                Ok(self.system_error_result(task, started_at, error.to_string()))
            }
            Err(error) => Err(TaskFailure::retry(error.to_string())),
        }
    }
}

/// The retry budget bounds how often a retryable failure may bounce back to
/// the queue before the task degrades into a SystemError verdict.
fn retry_budget_exhausted(retry_count: u32) -> bool {
    retry_count >= MAX_TASK_RETRIES
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use bytes::Bytes;
    use project_balloon_contracts::{
        JUDGE_RESULT_SCHEMA_VERSION, JudgeRunResult, JudgeTask, JudgeVerdict,
    };
    use project_balloon_test_support::valid_judge_task;

    use super::{
        ArtifactError, ArtifactPreparer, JudgeEngine, SandboxError, SandboxJudge,
        retry_budget_exhausted, truncate_utf8,
    };
    use crate::{
        artifacts::PreparedArtifacts,
        rabbit::{JudgeTaskHandler, TaskFailureKind},
        sandbox::SandboxJudgement,
    };

    #[test]
    fn retry_budget_degrades_only_after_exhaustion() {
        assert!(!retry_budget_exhausted(0));
        assert!(!retry_budget_exhausted(7));
        assert!(retry_budget_exhausted(8));
        assert!(retry_budget_exhausted(9));
    }

    #[test]
    fn truncate_utf8_cuts_on_char_boundaries() {
        assert_eq!(truncate_utf8("hello", 5), "hello");
        assert_eq!(truncate_utf8("hello", 4), "hell");
        assert_eq!(truncate_utf8("中文消息", 7), "中文");
        assert_eq!(truncate_utf8("中文", 0), "");
    }

    enum ArtifactOutcome {
        Prepared { interactor: Option<Bytes> },
        HashMismatch,
        TooLarge,
        CacheIo,
    }

    struct StubArtifacts {
        outcome: ArtifactOutcome,
        preflight_ok: bool,
    }

    impl StubArtifacts {
        fn prepared() -> Self {
            Self { outcome: ArtifactOutcome::Prepared { interactor: None }, preflight_ok: true }
        }

        fn resolve(&self) -> Result<PreparedArtifacts, ArtifactError> {
            match &self.outcome {
                ArtifactOutcome::Prepared { interactor } => Ok(PreparedArtifacts {
                    source: Bytes::from_static(b"submission-source-bytes"),
                    testdata_archive: PathBuf::from("/cache/judge-under-test/testdata.zip"),
                    interactor: interactor.clone(),
                }),
                ArtifactOutcome::HashMismatch => Err(ArtifactError::HashMismatch {
                    kind: "source",
                    expected: "deadbeef".to_owned(),
                    actual: "badfeed".to_owned(),
                }),
                ArtifactOutcome::TooLarge => Err(ArtifactError::TooLarge(64 * 1024 * 1024)),
                ArtifactOutcome::CacheIo => {
                    Err(ArtifactError::Io(std::io::Error::other("cache directory vanished")))
                }
            }
        }
    }

    #[async_trait::async_trait]
    impl ArtifactPreparer for StubArtifacts {
        async fn preflight(&self) -> Result<(), ArtifactError> {
            if self.preflight_ok { Ok(()) } else { Err(ArtifactError::Timeout) }
        }

        async fn prepare(&self, _task: &JudgeTask) -> Result<PreparedArtifacts, ArtifactError> {
            self.resolve()
        }
    }

    #[derive(Clone)]
    struct ObservedJudge {
        source: Vec<u8>,
        archive: PathBuf,
        interactor: Option<Vec<u8>>,
    }

    enum SandboxOutcome {
        Judged { verdict: JudgeVerdict },
        InvalidTestdata,
        SandboxApi,
    }

    struct StubSandbox {
        outcome: SandboxOutcome,
        preflight_ok: bool,
        observed: Mutex<Vec<ObservedJudge>>,
    }

    impl StubSandbox {
        fn judged(verdict: JudgeVerdict) -> Self {
            Self {
                outcome: SandboxOutcome::Judged { verdict },
                preflight_ok: true,
                observed: Mutex::new(Vec::new()),
            }
        }

        fn resolve(
            &self,
            source: &[u8],
            archive: &Path,
            interactor: Option<&[u8]>,
        ) -> Result<SandboxJudgement, SandboxError> {
            self.observed.lock().expect("observed judge lock").push(ObservedJudge {
                source: source.to_vec(),
                archive: archive.to_path_buf(),
                interactor: interactor.map(<[u8]>::to_vec),
            });
            match &self.outcome {
                SandboxOutcome::Judged { verdict } => Ok(SandboxJudgement {
                    verdict: *verdict,
                    total_time_ms: 1234,
                    peak_memory_kb: 5678,
                    compile_log: Some("g++ 12.2.0".to_owned()),
                    runs: vec![JudgeRunResult {
                        test_index: 1,
                        verdict: *verdict,
                        time_ms: 12,
                        memory_kb: 34,
                        exit_code: Some(0),
                        stderr_tail: None,
                    }],
                }),
                SandboxOutcome::InvalidTestdata => {
                    Err(SandboxError::InvalidTestdata("unpaired test cases".to_owned()))
                }
                SandboxOutcome::SandboxApi => {
                    Err(SandboxError::Api("docker daemon unreachable".to_owned()))
                }
            }
        }
    }

    #[async_trait::async_trait]
    impl SandboxJudge for StubSandbox {
        async fn preflight(&self) -> Result<(), SandboxError> {
            if self.preflight_ok { Ok(()) } else { Err(SandboxError::Api("no daemon".to_owned())) }
        }

        async fn judge(
            &self,
            _task: &JudgeTask,
            source: &[u8],
            archive: &std::path::Path,
            interactor: Option<&[u8]>,
        ) -> Result<SandboxJudgement, SandboxError> {
            self.resolve(source, archive, interactor)
        }
    }

    fn engine_with(
        artifacts: StubArtifacts,
        sandbox: StubSandbox,
    ) -> JudgeEngine<StubArtifacts, StubSandbox> {
        JudgeEngine { worker_id: "worker-under-test".to_owned(), artifacts, sandbox }
    }

    fn observed_judge(engine: &JudgeEngine<StubArtifacts, StubSandbox>) -> ObservedJudge {
        let observed = engine.sandbox.observed.lock().expect("observed judge lock");
        observed.first().expect("sandbox must have judged exactly once").clone()
    }

    #[tokio::test]
    async fn completed_judgements_flow_through_to_the_confirmed_result() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts {
                outcome: ArtifactOutcome::Prepared {
                    interactor: Some(Bytes::from_static(b"interactor-binary")),
                },
                preflight_ok: true,
            },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );

        let result = engine.handle(&task, 0).await.expect("completed judgement must not retry");

        assert_eq!(result.schema_version, JUDGE_RESULT_SCHEMA_VERSION);
        assert_eq!(result.message_id, task.judgement_id);
        assert_eq!(result.judgement_id, task.judgement_id);
        assert_eq!(result.submission_id, task.submission_id);
        assert_eq!(result.worker_id, "worker-under-test");
        assert_eq!(result.verdict, JudgeVerdict::Accepted);
        assert_eq!(result.total_time_ms, 1234);
        assert_eq!(result.peak_memory_kb, 5678);
        assert_eq!(result.compile_log.as_deref(), Some("g++ 12.2.0"));
        assert_eq!(result.runs.len(), 1);
        assert_eq!(result.runs[0].verdict, JudgeVerdict::Accepted);
        assert!(result.started_at <= result.completed_at);

        let seen = observed_judge(&engine);
        assert_eq!(seen.source, b"submission-source-bytes");
        assert_eq!(seen.archive, PathBuf::from("/cache/judge-under-test/testdata.zip"));
        assert_eq!(seen.interactor.as_deref(), Some(b"interactor-binary".as_slice()));
    }

    #[tokio::test]
    async fn sandbox_never_runs_when_artifact_preparation_fails() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts { outcome: ArtifactOutcome::HashMismatch, preflight_ok: true },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );

        let result = engine.handle(&task, 0).await.expect("hash mismatch must not retry");

        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        assert_eq!(result.total_time_ms, 0);
        assert_eq!(result.peak_memory_kb, 0);
        assert!(result.runs.is_empty());
        let compile_log = result.compile_log.expect("system error must carry the reason");
        assert!(compile_log.contains("SHA-256 mismatch"), "unexpected log: {compile_log}");
        assert!(compile_log.contains("deadbeef") && compile_log.contains("badfeed"));
        assert!(engine.sandbox.observed.lock().expect("observed judge lock").is_empty());
    }

    #[tokio::test]
    async fn oversized_artifacts_become_a_system_error_without_a_retry() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts { outcome: ArtifactOutcome::TooLarge, preflight_ok: true },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );

        let result = engine.handle(&task, 0).await.expect("size violations must not retry");

        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        let compile_log = result.compile_log.expect("system error must carry the reason");
        assert!(
            compile_log.contains("exceeds configured maximum"),
            "unexpected log: {compile_log}"
        );
    }

    #[tokio::test]
    async fn transient_artifact_failures_stay_retryable_within_the_budget() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts { outcome: ArtifactOutcome::CacheIo, preflight_ok: true },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );

        let failure = engine
            .handle(&task, 7)
            .await
            .expect_err("cache I/O within the budget must stay retryable");

        assert_eq!(failure.kind, TaskFailureKind::Retry);
    }

    #[tokio::test]
    async fn exhausted_artifact_failures_degrade_into_a_system_error() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts { outcome: ArtifactOutcome::CacheIo, preflight_ok: true },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );

        let result = engine.handle(&task, 8).await.expect("exhausted budget must not retry");

        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        let compile_log = result.compile_log.expect("system error must carry the reason");
        assert!(compile_log.contains("cache directory vanished"), "unexpected log: {compile_log}");
    }

    #[tokio::test]
    async fn invalid_testdata_becomes_a_system_error_without_a_retry() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts::prepared(),
            StubSandbox {
                outcome: SandboxOutcome::InvalidTestdata,
                preflight_ok: true,
                observed: Mutex::new(Vec::new()),
            },
        );

        let result = engine.handle(&task, 0).await.expect("invalid testdata must not retry");

        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        assert!(result.runs.is_empty());
        let compile_log = result.compile_log.expect("system error must carry the reason");
        assert!(
            compile_log.contains("test-data archive is invalid"),
            "unexpected log: {compile_log}"
        );
    }

    #[tokio::test]
    async fn transient_sandbox_failures_stay_retryable_within_the_budget() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts::prepared(),
            StubSandbox {
                outcome: SandboxOutcome::SandboxApi,
                preflight_ok: true,
                observed: Mutex::new(Vec::new()),
            },
        );

        let failure = engine
            .handle(&task, 7)
            .await
            .expect_err("sandbox API within the budget must stay retryable");

        assert_eq!(failure.kind, TaskFailureKind::Retry);
    }

    #[tokio::test]
    async fn exhausted_sandbox_failures_degrade_into_a_system_error() {
        let task = valid_judge_task();
        let engine = engine_with(
            StubArtifacts::prepared(),
            StubSandbox {
                outcome: SandboxOutcome::SandboxApi,
                preflight_ok: true,
                observed: Mutex::new(Vec::new()),
            },
        );

        let result = engine.handle(&task, 8).await.expect("exhausted budget must not retry");

        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        let compile_log = result.compile_log.expect("system error must carry the reason");
        assert!(compile_log.contains("docker daemon unreachable"), "unexpected log: {compile_log}");
    }

    #[tokio::test]
    async fn rejected_verdicts_flow_through_unchanged() {
        let task = valid_judge_task();
        let engine =
            engine_with(StubArtifacts::prepared(), StubSandbox::judged(JudgeVerdict::WrongAnswer));

        let result = engine.handle(&task, 0).await.expect("a verdict must not retry");

        assert_eq!(result.verdict, JudgeVerdict::WrongAnswer);
        assert_eq!(result.runs[0].verdict, JudgeVerdict::WrongAnswer);
    }

    #[tokio::test]
    async fn preflight_requires_both_artifacts_and_sandbox() {
        let both_ready =
            engine_with(StubArtifacts::prepared(), StubSandbox::judged(JudgeVerdict::Accepted));
        both_ready.preflight().await.expect("healthy dependencies must pass preflight");

        let broken_artifacts = engine_with(
            StubArtifacts {
                outcome: ArtifactOutcome::Prepared { interactor: None },
                preflight_ok: false,
            },
            StubSandbox::judged(JudgeVerdict::Accepted),
        );
        let failure = broken_artifacts
            .preflight()
            .await
            .expect_err("artifact preflight failure must fail the worker");
        assert_eq!(failure.kind, TaskFailureKind::Retry);

        let broken_sandbox = engine_with(
            StubArtifacts::prepared(),
            StubSandbox {
                outcome: SandboxOutcome::Judged { verdict: JudgeVerdict::Accepted },
                preflight_ok: false,
                observed: Mutex::new(Vec::new()),
            },
        );
        let failure = broken_sandbox
            .preflight()
            .await
            .expect_err("sandbox preflight failure must fail the worker");
        assert_eq!(failure.kind, TaskFailureKind::Retry);
    }
}
