use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PracticeSubmitMetadata {
    pub problem_id: i64,
    pub language: String,
    #[serde(default)]
    pub training_enrollment_id: Option<i64>,
}

impl PracticeSubmitMetadata {
    pub fn validate(
        self,
        original_filename: &str,
        source: bytes::Bytes,
    ) -> Result<(ValidatedSubmission, Option<i64>), AppError> {
        if self.training_enrollment_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("trainingEnrollmentId", "must be positive"));
        }
        let enrollment_id = self.training_enrollment_id;
        let command = SubmitMetadata { problem_id: self.problem_id, language: self.language }
            .validate(original_filename, source)?;
        Ok((command, enrollment_id))
    }
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

/// Stable first-slice similarity fingerprint: comments and formatting
/// whitespace are ignored while literals remain byte-exact.
pub fn source_fingerprint(source: &[u8]) -> String {
    let mut normalized = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        let byte = source[index];
        if byte == b'/' && source.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < source.len() && !matches!(source[index], b'\n' | b'\r') {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && source.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < source.len() && !(source[index] == b'*' && source[index + 1] == b'/')
            {
                index += 1;
            }
            index = (index + 2).min(source.len());
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let quote = byte;
            normalized.push(byte);
            index += 1;
            let mut escaped = false;
            while index < source.len() {
                let literal = source[index];
                normalized.push(literal);
                index += 1;
                if escaped {
                    escaped = false;
                } else if literal == b'\\' {
                    escaped = true;
                } else if literal == quote {
                    break;
                }
            }
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        normalized.push(byte);
        index += 1;
    }
    hex::encode(Sha256::digest(normalized))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSimilaritySignature {
    pub simhash: i64,
    pub token_count: i32,
}

pub fn source_similarity_signature(source: &[u8]) -> SourceSimilaritySignature {
    let tokens = similarity_tokens(source);
    let mut weights = [0_i32; 64];
    let width = tokens.len().min(5);
    for window in tokens.windows(width.max(1)) {
        let mut hasher = Sha256::new();
        for token in window {
            hasher.update(token);
            hasher.update([0]);
        }
        let digest = hasher.finalize();
        let mut prefix = [0_u8; 8];
        prefix.copy_from_slice(&digest[..8]);
        let hash = u64::from_be_bytes(prefix);
        for (bit, weight) in weights.iter_mut().enumerate() {
            if hash & (1_u64 << bit) == 0 {
                *weight -= 1;
            } else {
                *weight += 1;
            }
        }
    }
    let simhash = weights.iter().enumerate().fold(0_u64, |value, (bit, weight)| {
        if *weight > 0 { value | (1_u64 << bit) } else { value }
    });
    SourceSimilaritySignature {
        simhash: i64::from_be_bytes(simhash.to_be_bytes()),
        token_count: i32::try_from(tokens.len()).unwrap_or(i32::MAX).max(1),
    }
}

fn similarity_tokens(source: &[u8]) -> Vec<Vec<u8>> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < source.len() {
        let byte = source[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'/' && source.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < source.len() && !matches!(source[index], b'\n' | b'\r') {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && source.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < source.len() && !(source[index] == b'*' && source[index + 1] == b'/')
            {
                index += 1;
            }
            index = (index + 2).min(source.len());
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let quote = byte;
            index += 1;
            let mut escaped = false;
            while index < source.len() {
                let literal = source[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if literal == b'\\' {
                    escaped = true;
                } else if literal == quote {
                    break;
                }
            }
            tokens.push(if quote == b'"' { b"str".to_vec() } else { b"char".to_vec() });
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            index += 1;
            while index < source.len()
                && (source[index].is_ascii_alphanumeric() || source[index] == b'_')
            {
                index += 1;
            }
            let word = &source[start..index];
            tokens.push(if is_similarity_keyword(word) {
                word.to_ascii_lowercase()
            } else {
                b"id".to_vec()
            });
            continue;
        }
        if byte.is_ascii_digit() {
            index += 1;
            while index < source.len()
                && (source[index].is_ascii_alphanumeric() || matches!(source[index], b'.' | b'_'))
            {
                index += 1;
            }
            tokens.push(b"num".to_vec());
            continue;
        }
        tokens.push(vec![byte]);
        index += 1;
    }
    if tokens.is_empty() {
        tokens.push(b"empty".to_vec());
    }
    tokens
}

fn is_similarity_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"if"
            | b"else"
            | b"for"
            | b"while"
            | b"do"
            | b"switch"
            | b"case"
            | b"return"
            | b"break"
            | b"continue"
            | b"class"
            | b"struct"
            | b"enum"
            | b"fn"
            | b"def"
            | b"let"
            | b"const"
            | b"static"
            | b"public"
            | b"private"
            | b"protected"
            | b"import"
            | b"from"
            | b"include"
            | b"try"
            | b"catch"
            | b"throw"
            | b"throws"
            | b"new"
            | b"delete"
            | b"match"
            | b"async"
            | b"await"
    )
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
        if source.is_empty() || source.len() > 256 * 1024 {
            return Err(AppError::validation("source", "must contain between 1 byte and 256 KiB"));
        }
        let language = self.language.trim().to_ascii_lowercase();
        if language != "output" && std::str::from_utf8(&source).is_err() {
            return Err(AppError::validation("source", "must be valid UTF-8 text"));
        }
        let filename =
            original_filename.rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
        let allowed = match language.as_str() {
            "c" => &[".c"][..],
            "cpp" => &[".cpp", ".cc", ".cxx"][..],
            "java" => &[".java"][..],
            "python" => &[".py"][..],
            "output" => &[".zip"][..],
            _ => {
                return Err(AppError::validation(
                    "language",
                    "must be c, cpp, java, python, or output",
                ));
            }
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
        if language.as_ref().is_some_and(|value| {
            !matches!(value.as_str(), "c" | "cpp" | "java" | "python" | "output")
        }) {
            return Err(AppError::validation(
                "language",
                "must be c, cpp, java, python, or output",
            ));
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
    pub score_milli: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSubmissionSummary {
    pub id: i64,
    pub problem_id: i64,
    pub problem_slug: String,
    pub problem_title: String,
    pub training_enrollment_id: Option<i64>,
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
    pub score: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PracticeProblemStatus {
    pub problem_id: i64,
    pub attempts: i32,
    pub best_score: i32,
    pub solved: bool,
    pub last_submission_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub solved_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
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
    pub score_milli: Option<i32>,
    #[sqlx(skip)]
    pub runs: Vec<RunDetail>,
    #[sqlx(skip)]
    pub subtask_scores: Vec<JudgementSubtaskScore>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct JudgementSubtaskScore {
    pub subtask_key: String,
    pub name: String,
    pub score_milli: i32,
    pub max_score_milli: i32,
    pub passed_tests: i32,
    pub total_tests: i32,
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

    use super::{SubmitMetadata, source_fingerprint, source_similarity_signature};

    #[test]
    fn source_fingerprint_ignores_comments_and_formatting_but_keeps_literals() {
        let first = b"int main() { // comment\n return \"a b\"; }";
        let second = b"int  main(){/* other */return \"a b\";}";
        let different = b"int main(){return\"ab\";}";
        assert_eq!(source_fingerprint(first), source_fingerprint(second));
        assert_ne!(source_fingerprint(first), source_fingerprint(different));
    }

    #[test]
    fn simhash_stays_close_when_identifiers_and_literals_change() {
        let first = source_similarity_signature(
            b"int sum(int a,int b){return a+b;} int main(){return sum(2,3);}",
        );
        let renamed = source_similarity_signature(
            b"int add(int x,int y){ return x + y; } int main(){return add(8,9);}",
        );
        let unrelated = source_similarity_signature(b"for(int i=0;i<10;i++){while(true){break;}}");
        assert!((first.simhash ^ renamed.simhash).count_ones() <= 8);
        assert!((first.simhash ^ unrelated.simhash).count_ones() > 8);
    }

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
