use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitMetadata {
    pub problem_id: i64,
    pub language: String,
}

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
#[schema(as = SubmissionUploadRequest)]
pub struct SubmissionUploadRequest {
    #[schema(value_type = String)]
    pub metadata: String,
    #[schema(value_type = String, format = Binary)]
    pub source: Vec<u8>,
}

pub struct ValidatedSubmission {
    pub problem_id: i64,
    pub language: String,
    pub extension: &'static str,
    pub source: bytes::Bytes,
}

impl SubmitMetadata {
    pub fn validate(
        self,
        original_filename: &str,
        source: bytes::Bytes,
    ) -> Result<ValidatedSubmission, AppError> {
        if self.problem_id <= 0 {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if source.is_empty() || source.len() > 64 * 1024 {
            return Err(AppError::validation("source", "must contain between 1 byte and 64 KiB"));
        }
        if std::str::from_utf8(&source).is_err() {
            return Err(AppError::validation("source", "must be valid UTF-8 text"));
        }
        let language = self.language.trim().to_ascii_lowercase();
        let filename =
            original_filename.rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
        let allowed = match language.as_str() {
            "c" => &[".c"][..],
            "cpp" => &[".cpp", ".cc", ".cxx"][..],
            "java" => &[".java"][..],
            "python" => &[".py"][..],
            _ => return Err(AppError::validation("language", "must be c, cpp, java, or python")),
        };
        let extension =
            allowed.iter().copied().find(|extension| filename.ends_with(extension)).ok_or_else(
                || AppError::validation("source", "filename extension does not match language"),
            )?;
        Ok(ValidatedSubmission { problem_id: self.problem_id, language, extension, source })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitResponse {
    pub submission_id: i64,
    pub judgement_id: Uuid,
    pub status: &'static str,
    #[serde(with = "time::serde::rfc3339")]
    pub submitted_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RejudgeRequest {
    pub expected_judgement_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RejudgeResponse {
    pub submission_id: i64,
    pub previous_judgement_id: Uuid,
    pub judgement_id: Uuid,
    pub status: &'static str,
    #[serde(with = "time::serde::rfc3339")]
    pub queued_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JudgeQueueStatusResponse {
    pub contest_id: i64,
    pub drained: bool,
    pub pending_submissions: i64,
    pub judging_submissions: i64,
    pub outbox_pending: i64,
    pub outbox_failed: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub checked_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionListQuery {
    pub team_id: Option<i64>,
    pub problem_id: Option<i64>,
    pub status: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub sort: Option<String>,
}

const fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

pub struct ValidatedSubmissionListQuery {
    pub team_id: Option<i64>,
    pub problem_id: Option<i64>,
    pub status: Option<String>,
    pub language: Option<String>,
    pub page: u32,
    pub size: u32,
    pub offset: i64,
}

impl SubmissionListQuery {
    pub fn validate(self) -> Result<ValidatedSubmissionListQuery, AppError> {
        if self.team_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("teamId", "must be positive"));
        }
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if !(1..=MAX_PAGE_SIZE).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 100"));
        }
        if self.sort.as_deref().is_some_and(|sort| sort != "submittedAt,desc") {
            return Err(AppError::validation("sort", "only submittedAt,desc is supported"));
        }
        let status = self.status.map(|value| value.trim().to_ascii_uppercase());
        if status.as_ref().is_some_and(|value| {
            !matches!(
                value.as_str(),
                "PENDING"
                    | "JUDGING"
                    | "ACCEPTED"
                    | "WRONG_ANSWER"
                    | "COMPILE_ERROR"
                    | "RUNTIME_ERROR"
                    | "TIME_LIMIT_EXCEEDED"
                    | "MEMORY_LIMIT_EXCEEDED"
                    | "OUTPUT_LIMIT_EXCEEDED"
                    | "SYSTEM_ERROR"
                    | "CANCELLED"
            )
        }) {
            return Err(AppError::validation(
                "status",
                "contains an unsupported submission status",
            ));
        }
        let language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if language
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(AppError::validation("language", "must be c, cpp, java, or python"));
        }
        Ok(ValidatedSubmissionListQuery {
            team_id: self.team_id,
            problem_id: self.problem_id,
            status: status.filter(|value| !value.is_empty()),
            language: language.filter(|value| !value.is_empty()),
            page: self.page,
            size: self.size,
            offset: checked_offset(self.page, self.size)?,
        })
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionSummary {
    pub id: i64,
    pub contest_id: i64,
    pub problem_id: i64,
    pub problem_alias: String,
    pub team_id: i64,
    pub team_name: String,
    pub language: String,
    pub source_size_bytes: i32,
    pub status: String,
    #[serde(with = "time::serde::rfc3339")]
    pub submitted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub judged_at: Option<OffsetDateTime>,
    pub active_judgement_id: Option<Uuid>,
    pub verdict: Option<String>,
    pub total_time_ms: Option<i32>,
    pub peak_memory_kb: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionDetail {
    #[serde(flatten)]
    pub summary: SubmissionSummary,
    pub source: String,
    pub source_sha256: Option<String>,
    pub judgements: Vec<JudgementDetail>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct JudgementDetail {
    pub id: Uuid,
    pub verdict: Option<String>,
    pub total_time_ms: Option<i32>,
    pub peak_memory_kb: Option<i32>,
    pub compile_log: Option<String>,
    pub worker_id: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub version: i32,
    pub superseded: bool,
    pub active: bool,
    #[sqlx(skip)]
    pub runs: Vec<RunDetail>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub test_index: i32,
    pub verdict: Option<String>,
    pub time_ms: Option<i32>,
    pub memory_kb: Option<i32>,
    pub exit_code: Option<i32>,
    pub stderr_tail: Option<String>,
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::SubmitMetadata;

    #[test]
    fn source_language_extension_and_size_are_closed() {
        let valid = SubmitMetadata { problem_id: 1, language: " Cpp ".into() }
            .validate("main.CPP", Bytes::from_static(b"int main(){}"))
            .expect("valid C++ source");
        assert_eq!(valid.language, "cpp");
        assert_eq!(valid.extension, ".cpp");
        assert!(
            SubmitMetadata { problem_id: 1, language: "cpp".into() }
                .validate("main.py", Bytes::from_static(b"print(1)"))
                .is_err()
        );
        assert!(
            SubmitMetadata { problem_id: 1, language: "cpp".into() }
                .validate("main.cpp", Bytes::from_static(&[0xff, 0xfe]))
                .is_err()
        );
    }

    #[test]
    fn submission_filters_are_closed_and_bounded() {
        use super::SubmissionListQuery;

        let valid = SubmissionListQuery {
            team_id: Some(1),
            problem_id: Some(2),
            status: Some(" accepted ".into()),
            language: Some(" CPP ".into()),
            page: 1,
            size: 25,
            sort: Some("submittedAt,desc".into()),
        }
        .validate()
        .expect("valid submission filters");
        assert_eq!(valid.status.as_deref(), Some("ACCEPTED"));
        assert_eq!(valid.language.as_deref(), Some("cpp"));
        assert_eq!(valid.offset, 25);
        assert!(
            SubmissionListQuery {
                team_id: None,
                problem_id: None,
                status: Some("UNKNOWN".into()),
                language: None,
                page: 0,
                size: 25,
                sort: None,
            }
            .validate()
            .is_err()
        );
    }
}
