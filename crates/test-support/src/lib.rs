use project_balloon_contracts::{JUDGE_TASK_SCHEMA_VERSION, JudgeTask};
use uuid::Uuid;

#[must_use]
pub fn valid_judge_task() -> JudgeTask {
    JudgeTask {
        schema_version: JUDGE_TASK_SCHEMA_VERSION,
        judgement_id: Uuid::from_u128(1),
        submission_id: 42,
        problem_id: 7,
        testdata_version: 2,
        testdata_object_key: "problems/7/v2.zip".to_owned(),
        testdata_sha256: "a".repeat(64),
        source_object_key: "submissions/42/main.cpp".to_owned(),
        source_sha256: "b".repeat(64),
        language: "cpp".to_owned(),
        time_limit_ms: 1_000,
        memory_limit_mb: 256,
        output_limit_kb: 64,
        language_multiplier: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::valid_judge_task;

    #[test]
    fn fixture_is_valid() {
        assert!(valid_judge_task().validate().is_ok());
    }
}
