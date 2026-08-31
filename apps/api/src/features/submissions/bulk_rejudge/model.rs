use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::AppError;

const MAX_BATCH_ITEMS: i32 = 10_000;

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRejudgeFilter {
    pub problem_id: Option<i64>,
    pub team_id: Option<i64>,
    pub language: Option<String>,
    pub verdict: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub submitted_from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub submitted_to: Option<OffsetDateTime>,
}

impl BatchRejudgeFilter {
    pub(super) fn validate(mut self) -> Result<Self, AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if self.team_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("teamId", "must be positive"));
        }
        if self.submitted_from.zip(self.submitted_to).is_some_and(|(from, to)| from > to) {
            return Err(AppError::validation("submittedFrom", "must not be after submittedTo"));
        }
        self.language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if self
            .language
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(AppError::validation("language", "must be c, cpp, java, or python"));
        }
        self.verdict = self.verdict.map(|value| value.trim().to_ascii_uppercase());
        const VERDICTS: &[&str] = &[
            "ACCEPTED",
            "WRONG_ANSWER",
            "COMPILE_ERROR",
            "RUNTIME_ERROR",
            "TIME_LIMIT_EXCEEDED",
            "MEMORY_LIMIT_EXCEEDED",
            "OUTPUT_LIMIT_EXCEEDED",
            "SYSTEM_ERROR",
            "CANCELLED",
        ];
        if self.verdict.as_ref().is_some_and(|value| !VERDICTS.contains(&value.as_str())) {
            return Err(AppError::validation("verdict", "contains an unsupported final verdict"));
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRejudgeCreateRequest {
    pub filter: BatchRejudgeFilter,
    pub expected_count: i32,
    pub confirmation_text: String,
    pub idempotency_key: String,
}

impl BatchRejudgeCreateRequest {
    pub(super) fn validate(mut self) -> Result<Self, AppError> {
        self.filter = self.filter.validate()?;
        if !(1..=MAX_BATCH_ITEMS).contains(&self.expected_count) {
            return Err(AppError::validation("expectedCount", "must be between 1 and 10000"));
        }
        if self.confirmation_text != format!("REJUDGE {}", self.expected_count) {
            return Err(AppError::validation(
                "confirmationText",
                "must equal REJUDGE followed by expectedCount",
            ));
        }
        self.idempotency_key = self.idempotency_key.trim().to_owned();
        if !(8..=128).contains(&self.idempotency_key.len()) {
            return Err(AppError::validation("idempotencyKey", "must contain 8 to 128 bytes"));
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgePreviewResponse {
    pub matched_submissions: i32,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgeItemResponse {
    pub id: i64,
    pub submission_id: i64,
    pub status: String,
    pub old_judgement_id: Option<Uuid>,
    pub new_judgement_id: Option<Uuid>,
    pub error_message: Option<String>,
    pub attempts: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    pub processed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRejudgeTaskResponse {
    pub id: i64,
    pub contest_id: i64,
    pub status: String,
    pub total_items: i32,
    pub processed_items: i32,
    pub succeeded_items: i32,
    pub failed_items: i32,
    pub cancel_requested: bool,
    pub created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub items: Vec<BatchRejudgeItemResponse>,
    pub items_truncated: bool,
}

#[derive(sqlx::FromRow)]
pub(super) struct BatchRejudgeTaskRow {
    id: i64,
    contest_id: i64,
    status: String,
    total_items: i32,
    processed_items: i32,
    succeeded_items: i32,
    failed_items: i32,
    cancel_requested: bool,
    created_by_user_id: i64,
    started_at: Option<OffsetDateTime>,
    completed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl BatchRejudgeTaskRow {
    pub(super) fn response(self) -> BatchRejudgeTaskResponse {
        BatchRejudgeTaskResponse {
            id: self.id,
            contest_id: self.contest_id,
            status: self.status,
            total_items: self.total_items,
            processed_items: self.processed_items,
            succeeded_items: self.succeeded_items,
            failed_items: self.failed_items,
            cancel_requested: self.cancel_requested,
            created_by_user_id: self.created_by_user_id,
            started_at: self.started_at,
            completed_at: self.completed_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            items: Vec::new(),
            items_truncated: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchRejudgeCreateRequest, BatchRejudgeFilter};

    fn filter() -> BatchRejudgeFilter {
        BatchRejudgeFilter {
            problem_id: None,
            team_id: None,
            language: None,
            verdict: None,
            submitted_from: None,
            submitted_to: None,
        }
    }

    fn request(
        expected_count: i32,
        confirmation_text: &str,
        idempotency_key: &str,
    ) -> BatchRejudgeCreateRequest {
        BatchRejudgeCreateRequest {
            filter: filter(),
            expected_count,
            confirmation_text: confirmation_text.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
        }
    }

    #[test]
    fn well_formed_batch_requests_pass_validation() {
        request(1, "REJUDGE 1", "k".repeat(8).as_str()).validate().expect("lower bounds");
        request(10_000, "REJUDGE 10000", "k".repeat(128).as_str())
            .validate()
            .expect("upper bounds");
    }

    #[test]
    fn expected_count_must_stay_within_batch_limits() {
        for count in [0, 10_001] {
            assert!(
                request(count, &format!("REJUDGE {count}"), "idempotency-key").validate().is_err(),
                "count {count} must be rejected"
            );
        }
    }

    #[test]
    fn confirmation_text_must_echo_the_expected_count() {
        assert!(request(3, "REJUDGE 4", "idempotency-key").validate().is_err());
        assert!(request(3, "rejudge 3", "idempotency-key").validate().is_err());
    }

    #[test]
    fn idempotency_keys_are_trimmed_and_length_bounded() {
        assert!(request(1, "REJUDGE 1", "k".repeat(7).as_str()).validate().is_err());
        assert!(request(1, "REJUDGE 1", "k".repeat(129).as_str()).validate().is_err());
        let validated =
            request(1, "REJUDGE 1", "   idempotency-key   ").validate().expect("trimmed key");
        assert_eq!(validated.idempotency_key, "idempotency-key");
    }

    #[test]
    fn filter_languages_and_verdicts_are_normalized_and_bounded() {
        let validated_filter = BatchRejudgeFilter {
            language: Some(" CPP ".to_owned()),
            verdict: Some("wrong_answer".to_owned()),
            ..filter()
        }
        .validate()
        .expect("valid filter");
        assert_eq!(validated_filter.language.as_deref(), Some("cpp"));
        assert_eq!(validated_filter.verdict.as_deref(), Some("WRONG_ANSWER"));
        assert!(
            BatchRejudgeFilter { language: Some("rust".to_owned()), ..filter() }
                .validate()
                .is_err()
        );
    }
}
