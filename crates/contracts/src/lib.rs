use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub const JUDGE_TASK_SCHEMA_VERSION: u16 = 1;
pub const JUDGE_RESULT_SCHEMA_VERSION: u16 = 1;
pub const REALTIME_EVENT_SCHEMA_VERSION: u16 = 1;
pub const JUDGE_TASKS_QUEUE: &str = "judge.tasks";
pub const JUDGE_TASKS_EXCHANGE: &str = "judge.tasks.exchange";
pub const JUDGE_RETRY_QUEUE: &str = "judge.retry";
pub const JUDGE_RETRY_EXCHANGE: &str = "judge.retry.exchange";
pub const JUDGE_DEAD_QUEUE: &str = "judge.dead";
pub const JUDGE_RESULTS_EXCHANGE: &str = "judge.results.exchange";
pub const JUDGE_RESULTS_QUEUE: &str = "judge.results";
pub const JUDGE_RESULT_ROUTING_KEY: &str = "result";
pub const JUDGE_DEAD_EXCHANGE: &str = "judge.dead.exchange";
pub const JUDGE_DEAD_ROUTING_KEY: &str = "dead";
pub const WORKER_HEARTBEAT_SCHEMA_VERSION: u16 = 1;
pub const JUDGE_HEARTBEATS_QUEUE: &str = "judge.heartbeats";
pub const JUDGE_HEARTBEATS_EXCHANGE: &str = "judge.heartbeats.exchange";
pub const JUDGE_HEARTBEAT_ROUTING_KEY: &str = "heartbeat";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JudgeMode {
    #[default]
    Standard,
    Interactive,
    OutputOnly,
}

impl JudgeMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "STANDARD",
            Self::Interactive => "INTERACTIVE",
            Self::OutputOnly => "OUTPUT_ONLY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RealtimeScope {
    Public,
    Staff,
    Team,
}

impl RealtimeScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "PUBLIC",
            Self::Staff => "STAFF",
            Self::Team => "TEAM",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEvent {
    pub id: Uuid,
    pub version: u16,
    #[serde(rename = "type")]
    pub event_type: String,
    pub scope: RealtimeScope,
    pub contest_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub payload: Value,
}

impl RealtimeEvent {
    #[must_use]
    pub fn connected(contest_id: i64, scope: RealtimeScope) -> Self {
        Self {
            id: Uuid::new_v4(),
            version: REALTIME_EVENT_SCHEMA_VERSION,
            event_type: "CONNECTED".to_owned(),
            scope,
            contest_id,
            occurred_at: OffsetDateTime::now_utc(),
            payload: Value::Object(serde_json::Map::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeTask {
    #[serde(
        default = "judge_task_schema_version",
        skip_serializing_if = "is_legacy_schema_version"
    )]
    pub schema_version: u16,
    pub judgement_id: Uuid,
    pub submission_id: i64,
    pub problem_id: i64,
    pub testdata_version: i32,
    pub testdata_object_key: String,
    pub testdata_sha256: String,
    pub source_object_key: String,
    pub source_sha256: String,
    pub language: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub language_multiplier: f64,
    #[serde(default)]
    pub judge_mode: JudgeMode,
    #[serde(default)]
    pub interactor_object_key: Option<String>,
    #[serde(default)]
    pub interactor_sha256: Option<String>,
}

impl JudgeTask {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != JUDGE_TASK_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchemaVersion(self.schema_version));
        }
        for (name, value) in [
            ("submissionId", self.submission_id),
            ("problemId", self.problem_id),
            ("testdataVersion", i64::from(self.testdata_version)),
            ("timeLimitMs", i64::from(self.time_limit_ms)),
            ("memoryLimitMb", i64::from(self.memory_limit_mb)),
            ("outputLimitKb", i64::from(self.output_limit_kb)),
        ] {
            if value <= 0 {
                return Err(ContractError::NonPositive { name, value });
            }
        }
        if !self.language_multiplier.is_finite() || self.language_multiplier <= 0.0 {
            return Err(ContractError::InvalidLanguageMultiplier);
        }
        for (name, value) in [
            ("testdataObjectKey", self.testdata_object_key.as_str()),
            ("sourceObjectKey", self.source_object_key.as_str()),
        ] {
            if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err(ContractError::InvalidObjectKey(name));
            }
        }
        for (name, value) in [
            ("testdataSha256", self.testdata_sha256.as_str()),
            ("sourceSha256", self.source_sha256.as_str()),
        ] {
            if value.len() != 64
                || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ContractError::InvalidSha256(name));
            }
        }
        if !matches!(self.language.as_str(), "c" | "cpp" | "java" | "python" | "output") {
            return Err(ContractError::UnsupportedLanguage);
        }
        if self.judge_mode == JudgeMode::Interactive {
            let valid_interactor = self.interactor_object_key.as_ref().is_some_and(|key| {
                !key.is_empty() && key.len() <= 512 && !key.chars().any(char::is_control)
            }) && self.interactor_sha256.as_ref().is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            });
            if !valid_interactor {
                return Err(ContractError::InvalidObjectKey("interactorObjectKey"));
            }
        } else if self.interactor_object_key.is_some() || self.interactor_sha256.is_some() {
            return Err(ContractError::InvalidObjectKey("interactorObjectKey"));
        }
        if self.judge_mode == JudgeMode::OutputOnly && self.language != "output" {
            return Err(ContractError::UnsupportedLanguage);
        }
        if self.judge_mode != JudgeMode::OutputOnly && self.language == "output" {
            return Err(ContractError::UnsupportedLanguage);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JudgeVerdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    CompileError,
    OutputLimitExceeded,
    SystemError,
    Cancelled,
}

impl JudgeVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "ACCEPTED",
            Self::WrongAnswer => "WRONG_ANSWER",
            Self::TimeLimitExceeded => "TIME_LIMIT_EXCEEDED",
            Self::MemoryLimitExceeded => "MEMORY_LIMIT_EXCEEDED",
            Self::RuntimeError => "RUNTIME_ERROR",
            Self::CompileError => "COMPILE_ERROR",
            Self::OutputLimitExceeded => "OUTPUT_LIMIT_EXCEEDED",
            Self::SystemError => "SYSTEM_ERROR",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRunResult {
    pub test_index: i32,
    pub verdict: JudgeVerdict,
    pub time_ms: i32,
    pub memory_kb: i32,
    pub exit_code: Option<i32>,
    pub stderr_tail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeResult {
    pub schema_version: u16,
    pub message_id: Uuid,
    pub judgement_id: Uuid,
    pub submission_id: i64,
    pub worker_id: String,
    pub verdict: JudgeVerdict,
    pub total_time_ms: i32,
    pub peak_memory_kb: i32,
    pub compile_log: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: OffsetDateTime,
    pub runs: Vec<JudgeRunResult>,
}

impl JudgeResult {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != JUDGE_RESULT_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.message_id != self.judgement_id {
            return Err(ContractError::MismatchedResultMessageId);
        }
        if self.submission_id <= 0 {
            return Err(ContractError::NonPositive {
                name: "submissionId",
                value: self.submission_id,
            });
        }
        if self.worker_id.is_empty()
            || self.worker_id.len() > 64
            || self.worker_id.chars().any(char::is_control)
        {
            return Err(ContractError::InvalidWorkerId);
        }
        if self.total_time_ms < 0 || self.peak_memory_kb < 0 {
            return Err(ContractError::NegativeMetric);
        }
        if self.completed_at < self.started_at {
            return Err(ContractError::InvalidResultTimeline);
        }
        if self.compile_log.as_ref().is_some_and(|log| log.len() > 64 * 1024) {
            return Err(ContractError::CompileLogTooLarge);
        }
        if self.runs.len() > 10_000 {
            return Err(ContractError::TooManyRuns);
        }
        let mut test_indexes = std::collections::HashSet::with_capacity(self.runs.len());
        for run in &self.runs {
            if run.test_index <= 0 {
                return Err(ContractError::NonPositive {
                    name: "testIndex",
                    value: i64::from(run.test_index),
                });
            }
            if run.time_ms < 0 || run.memory_kb < 0 {
                return Err(ContractError::NegativeMetric);
            }
            if !test_indexes.insert(run.test_index) {
                return Err(ContractError::DuplicateTestIndex(run.test_index));
            }
            if run.stderr_tail.as_ref().is_some_and(|tail| tail.len() > 16 * 1024) {
                return Err(ContractError::StderrTailTooLarge);
            }
        }
        if self.runs.is_empty() {
            if !matches!(
                self.verdict,
                JudgeVerdict::CompileError | JudgeVerdict::SystemError | JudgeVerdict::Cancelled
            ) {
                return Err(ContractError::MissingRunsForVerdict);
            }
        } else {
            let expected_verdict = self
                .runs
                .iter()
                .find(|run| run.verdict != JudgeVerdict::Accepted)
                .map_or(JudgeVerdict::Accepted, |run| run.verdict);
            if self.verdict != expected_verdict {
                return Err(ContractError::MismatchedResultVerdict);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerHeartbeat {
    pub schema_version: u16,
    pub message_id: Uuid,
    pub worker_id: String,
    pub instance_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    pub capacity: u16,
    pub active_tasks: u16,
    pub languages: Vec<String>,
    pub runtime_versions: BTreeMap<String, String>,
    pub sandbox_runtime: Option<String>,
}

impl WorkerHeartbeat {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != WORKER_HEARTBEAT_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.worker_id.is_empty() || self.worker_id.len() > 64 {
            return Err(ContractError::InvalidWorkerId);
        }
        if self.capacity == 0 || self.active_tasks > self.capacity {
            return Err(ContractError::InvalidWorkerCapacity);
        }
        if self.occurred_at < self.started_at {
            return Err(ContractError::InvalidHeartbeatTimeline);
        }
        if self.languages.is_empty()
            || self.languages.len() > 16
            || self
                .languages
                .iter()
                .any(|language| !matches!(language.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(ContractError::InvalidWorkerLanguages);
        }
        if self.runtime_versions.len() > 16
            || self.runtime_versions.iter().any(|(name, version)| {
                name.is_empty()
                    || name.len() > 32
                    || version.is_empty()
                    || version.len() > 128
                    || name.chars().any(char::is_control)
                    || version.chars().any(char::is_control)
            })
        {
            return Err(ContractError::InvalidRuntimeVersions);
        }
        if self
            .sandbox_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.is_empty() || runtime.len() > 64)
        {
            return Err(ContractError::InvalidSandboxRuntime);
        }
        Ok(())
    }
}

const fn judge_task_schema_version() -> u16 {
    JUDGE_TASK_SCHEMA_VERSION
}

const fn is_legacy_schema_version(version: &u16) -> bool {
    *version == JUDGE_TASK_SCHEMA_VERSION
}

#[derive(Debug, Error, PartialEq)]
pub enum ContractError {
    #[error("unsupported judge task schema version {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("judge result messageId must equal judgementId")]
    MismatchedResultMessageId,
    #[error("{name} must be positive, got {value}")]
    NonPositive { name: &'static str, value: i64 },
    #[error("languageMultiplier must be finite and positive")]
    InvalidLanguageMultiplier,
    #[error("{0} must be a nonempty bounded object key")]
    InvalidObjectKey(&'static str),
    #[error("{0} must be a lowercase SHA-256 digest")]
    InvalidSha256(&'static str),
    #[error("Judge task language is unsupported")]
    UnsupportedLanguage,
    #[error("workerId must contain between 1 and 64 bytes")]
    InvalidWorkerId,
    #[error("judge result metrics must be nonnegative")]
    NegativeMetric,
    #[error("judge result completedAt precedes startedAt")]
    InvalidResultTimeline,
    #[error("compileLog exceeds 64 KiB")]
    CompileLogTooLarge,
    #[error("judge result contains more than 10000 runs")]
    TooManyRuns,
    #[error("duplicate testIndex {0}")]
    DuplicateTestIndex(i32),
    #[error("stderrTail exceeds 16 KiB")]
    StderrTailTooLarge,
    #[error("judge result with a non-compilation verdict must contain runs")]
    MissingRunsForVerdict,
    #[error("judge result verdict does not match its runs")]
    MismatchedResultVerdict,
    #[error("Worker capacity must be positive and activeTasks cannot exceed it")]
    InvalidWorkerCapacity,
    #[error("Worker heartbeat occurredAt precedes startedAt")]
    InvalidHeartbeatTimeline,
    #[error("Worker languages must be a nonempty subset of the P0 language set")]
    InvalidWorkerLanguages,
    #[error("Worker runtime versions are invalid")]
    InvalidRuntimeVersions,
    #[error("Worker sandbox runtime is invalid")]
    InvalidSandboxRuntime,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::{
        JUDGE_RESULT_SCHEMA_VERSION, JUDGE_TASK_SCHEMA_VERSION, JudgeResult, JudgeRunResult,
        JudgeTask, JudgeVerdict, REALTIME_EVENT_SCHEMA_VERSION, RealtimeEvent, RealtimeScope,
        WORKER_HEARTBEAT_SCHEMA_VERSION, WorkerHeartbeat,
    };

    #[test]
    fn legacy_java_message_without_version_deserializes_as_v1() {
        let judgement_id = Uuid::new_v4();
        let value = json!({
            "judgementId": judgement_id,
            "submissionId": 42,
            "problemId": 7,
            "testdataVersion": 2,
            "testdataObjectKey": "problems/7/v2.zip",
            "testdataSha256": "a".repeat(64),
            "sourceObjectKey": "submissions/42/main.cpp",
            "sourceSha256": "b".repeat(64),
            "language": "cpp",
            "timeLimitMs": 1000,
            "memoryLimitMb": 256,
            "outputLimitKb": 64,
            "languageMultiplier": 1.0
        });

        let task: JudgeTask =
            serde_json::from_value(value).expect("legacy message must deserialize");

        assert_eq!(task.schema_version, JUDGE_TASK_SCHEMA_VERSION);
        assert_eq!(task.judgement_id, judgement_id);
        assert!(task.validate().is_ok());
        let serialized = serde_json::to_value(task).expect("task must serialize");
        assert!(
            serialized.get("schemaVersion").is_none(),
            "v1 must remain compatible with the previous consumer"
        );
    }

    #[test]
    fn realtime_event_matches_the_existing_browser_contract() {
        let event = RealtimeEvent::connected(42, RealtimeScope::Public);
        let value = serde_json::to_value(event).expect("event must serialize");

        assert_eq!(value["version"], REALTIME_EVENT_SCHEMA_VERSION);
        assert_eq!(value["type"], "CONNECTED");
        assert_eq!(value["scope"], "PUBLIC");
        assert_eq!(value["contestId"], 42);
        assert!(value.get("occurredAt").is_some());
    }

    #[test]
    fn judge_result_has_closed_verdicts_and_rejects_duplicate_runs() {
        let now = time::OffsetDateTime::now_utc();
        let run = JudgeRunResult {
            test_index: 1,
            verdict: JudgeVerdict::Accepted,
            time_ms: 12,
            memory_kb: 1_024,
            exit_code: Some(0),
            stderr_tail: None,
        };
        let mut result = JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: Uuid::new_v4(),
            judgement_id: Uuid::new_v4(),
            submission_id: 42,
            worker_id: "worker-1".to_owned(),
            verdict: JudgeVerdict::Accepted,
            total_time_ms: 12,
            peak_memory_kb: 1_024,
            compile_log: None,
            started_at: now,
            completed_at: now,
            runs: vec![run.clone()],
        };
        result.message_id = result.judgement_id;
        assert!(result.validate().is_ok());
        assert_eq!(serde_json::to_value(&result).expect("serialize")["verdict"], "ACCEPTED");

        result.runs.push(run);
        assert!(matches!(result.validate(), Err(super::ContractError::DuplicateTestIndex(1))));
    }

    #[test]
    fn judge_result_rejects_mismatched_message_and_verdict_ids() {
        let now = time::OffsetDateTime::now_utc();
        let judgement_id = Uuid::new_v4();
        let mut result = JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: Uuid::new_v4(),
            judgement_id,
            submission_id: 42,
            worker_id: "worker-1".to_owned(),
            verdict: JudgeVerdict::Accepted,
            total_time_ms: 1,
            peak_memory_kb: 1,
            compile_log: None,
            started_at: now,
            completed_at: now,
            runs: vec![JudgeRunResult {
                test_index: 1,
                verdict: JudgeVerdict::WrongAnswer,
                time_ms: 1,
                memory_kb: 1,
                exit_code: Some(0),
                stderr_tail: None,
            }],
        };
        assert!(matches!(result.validate(), Err(super::ContractError::MismatchedResultMessageId)));
        result.message_id = judgement_id;
        assert!(matches!(result.validate(), Err(super::ContractError::MismatchedResultVerdict)));
    }

    #[test]
    fn judge_result_requires_runs_for_runtime_verdicts_and_rejects_controlled_worker_ids() {
        let now = time::OffsetDateTime::now_utc();
        let judgement_id = Uuid::new_v4();
        let mut result = JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: judgement_id,
            judgement_id,
            submission_id: 42,
            worker_id: "worker-1".to_owned(),
            verdict: JudgeVerdict::WrongAnswer,
            total_time_ms: 0,
            peak_memory_kb: 0,
            compile_log: None,
            started_at: now,
            completed_at: now,
            runs: Vec::new(),
        };
        assert!(matches!(result.validate(), Err(super::ContractError::MissingRunsForVerdict)));

        result.verdict = JudgeVerdict::CompileError;
        assert!(result.validate().is_ok());
        result.worker_id = "worker-\n1".to_owned();
        assert!(matches!(result.validate(), Err(super::ContractError::InvalidWorkerId)));
    }

    #[test]
    fn worker_heartbeat_rejects_overcommitted_capacity() {
        let now = time::OffsetDateTime::now_utc();
        let mut heartbeat = WorkerHeartbeat {
            schema_version: WORKER_HEARTBEAT_SCHEMA_VERSION,
            message_id: Uuid::new_v4(),
            worker_id: "worker-1".to_owned(),
            instance_id: Uuid::new_v4(),
            started_at: now,
            occurred_at: now,
            capacity: 1,
            active_tasks: 0,
            languages: vec!["c".to_owned(), "cpp".to_owned()],
            runtime_versions: std::collections::BTreeMap::from([(
                "cpp".to_owned(),
                "12.2.0".to_owned(),
            )]),
            sandbox_runtime: Some("runsc".to_owned()),
        };
        assert!(heartbeat.validate().is_ok());
        heartbeat.active_tasks = 2;
        assert!(matches!(heartbeat.validate(), Err(super::ContractError::InvalidWorkerCapacity)));
    }
}
