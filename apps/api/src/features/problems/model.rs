use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_TIME_LIMIT_MS: i32 = 1_000;
const DEFAULT_MEMORY_LIMIT_MB: i32 = 256;
const DEFAULT_OUTPUT_LIMIT_KB: i32 = 65_536;
const DEFAULT_LANG_CODE: &str = "en";
const P0_LANGUAGES: [&str; 4] = ["c", "cpp", "java", "python"];

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemListQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub contest_id: Option<i64>,
}

const fn default_page_size() -> u32 {
    50
}

impl ProblemListQuery {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(1..=100).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 100"));
        }
        if self.contest_id.is_some_and(|contest_id| contest_id <= 0) {
            return Err(AppError::validation("contestId", "must be positive"));
        }
        Ok(())
    }

    pub fn offset(&self) -> Result<i64, AppError> {
        checked_offset(self.page, self.size)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProblemRequest {
    pub slug: String,
    pub title: String,
    pub time_limit_ms: Option<i32>,
    pub memory_limit_mb: Option<i32>,
    pub output_limit_kb: Option<i32>,
    pub languages: Option<Vec<String>>,
    pub default_lang_code: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProblemRequest {
    pub expected_version: i64,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub time_limit_ms: Option<i32>,
    pub memory_limit_mb: Option<i32>,
    pub output_limit_kb: Option<i32>,
    pub languages: Option<Vec<String>>,
    pub default_lang_code: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertStatementRequest {
    pub body: String,
}

pub struct ValidatedStatement {
    pub lang_code: String,
    pub body: String,
}

impl UpsertStatementRequest {
    pub fn validate(self, lang_code: String) -> Result<ValidatedStatement, AppError> {
        let lang_code = validate_lang_code_field("langCode", lang_code)?;
        if self.body.trim().is_empty() {
            return Err(AppError::validation("body", "must not be blank"));
        }
        if self.body.len() > 1024 * 1024 {
            return Err(AppError::validation("body", "must not exceed 1 MiB"));
        }
        Ok(ValidatedStatement { lang_code, body: self.body })
    }
}

pub struct ValidatedProblem {
    pub slug: String,
    pub title: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub languages_json: String,
    pub default_lang_code: String,
}

pub struct ValidatedProblemUpdate {
    pub expected_version: i64,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub time_limit_ms: Option<i32>,
    pub memory_limit_mb: Option<i32>,
    pub output_limit_kb: Option<i32>,
    pub languages_json: Option<String>,
    pub default_lang_code: Option<String>,
}

impl CreateProblemRequest {
    pub fn validate(self) -> Result<ValidatedProblem, AppError> {
        let slug = validate_slug(self.slug)?;
        let title = validate_title(self.title)?;
        let time_limit_ms = validate_limit(
            "timeLimitMs",
            self.time_limit_ms.unwrap_or(DEFAULT_TIME_LIMIT_MS),
            1,
            60_000,
        )?;
        let memory_limit_mb = validate_limit(
            "memoryLimitMb",
            self.memory_limit_mb.unwrap_or(DEFAULT_MEMORY_LIMIT_MB),
            16,
            8_192,
        )?;
        let output_limit_kb = validate_limit(
            "outputLimitKb",
            self.output_limit_kb.unwrap_or(DEFAULT_OUTPUT_LIMIT_KB),
            1,
            262_144,
        )?;
        let languages_json = validate_languages(
            self.languages.unwrap_or_else(|| P0_LANGUAGES.map(str::to_owned).to_vec()),
        )?;
        let default_lang_code =
            validate_lang_code(self.default_lang_code.unwrap_or_else(|| DEFAULT_LANG_CODE.into()))?;
        Ok(ValidatedProblem {
            slug,
            title,
            time_limit_ms,
            memory_limit_mb,
            output_limit_kb,
            languages_json,
            default_lang_code,
        })
    }
}

impl UpdateProblemRequest {
    pub fn validate(self) -> Result<ValidatedProblemUpdate, AppError> {
        if self.expected_version < 0 {
            return Err(AppError::validation("expectedVersion", "must not be negative"));
        }
        if self.slug.is_none()
            && self.title.is_none()
            && self.time_limit_ms.is_none()
            && self.memory_limit_mb.is_none()
            && self.output_limit_kb.is_none()
            && self.languages.is_none()
            && self.default_lang_code.is_none()
        {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        Ok(ValidatedProblemUpdate {
            expected_version: self.expected_version,
            slug: self.slug.map(validate_slug).transpose()?,
            title: self.title.map(validate_title).transpose()?,
            time_limit_ms: self
                .time_limit_ms
                .map(|value| validate_limit("timeLimitMs", value, 1, 60_000))
                .transpose()?,
            memory_limit_mb: self
                .memory_limit_mb
                .map(|value| validate_limit("memoryLimitMb", value, 16, 8_192))
                .transpose()?,
            output_limit_kb: self
                .output_limit_kb
                .map(|value| validate_limit("outputLimitKb", value, 1, 262_144))
                .transpose()?,
            languages_json: self.languages.map(validate_languages).transpose()?,
            default_lang_code: self.default_lang_code.map(validate_lang_code).transpose()?,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemResponse {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub languages: Vec<String>,
    pub testdata_version: i32,
    pub testdata_sha256: Option<String>,
    pub default_lang_code: String,
    pub created_by: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub version: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemStatementResponse {
    pub problem_id: i64,
    pub lang_code: String,
    pub body: String,
    pub rendered_html: String,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy)]
pub enum AttachmentKind {
    Sample,
    Supplement,
}

impl AttachmentKind {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "SAMPLE" => Ok(Self::Sample),
            "SUPPLEMENT" => Ok(Self::Supplement),
            _ => Err(AppError::validation("kind", "must be SAMPLE or SUPPLEMENT")),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sample => "SAMPLE",
            Self::Supplement => "SUPPLEMENT",
        }
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProblemAttachmentResponse {
    pub id: i64,
    pub problem_id: i64,
    pub kind: String,
    pub original_filename: String,
    pub content_type: Option<String>,
    pub bytes: i64,
    pub sha256: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProblemTestdataResponse {
    pub problem_id: i64,
    pub version: i32,
    pub case_count: Option<i32>,
    pub bytes: Option<i64>,
    pub sha256: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProblemTestdataVersionResponse {
    pub problem_id: i64,
    pub version: i32,
    pub case_count: Option<i32>,
    pub bytes: Option<i64>,
    pub sha256: String,
    pub uploaded_by_user_id: Option<i64>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivateTestdataVersionRequest {
    pub expected_current_version: i32,
}

#[derive(Debug, ToSchema)]
#[schema(as = AttachmentUploadRequest)]
#[allow(dead_code)]
pub struct AttachmentUploadRequest {
    #[schema(example = "SAMPLE")]
    pub kind: String,
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, ToSchema)]
#[schema(as = TestdataUploadRequest)]
#[allow(dead_code)]
pub struct TestdataUploadRequest {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

pub fn validate_attachment_filename(value: String) -> Result<String, AppError> {
    let value = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if value.is_empty() || value.chars().count() > 255 || matches!(value.as_str(), "." | "..") {
        Err(AppError::validation("file", "must have a safe filename of at most 255 characters"))
    } else {
        Ok(value)
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ProblemStatementRow {
    pub problem_id: i64,
    pub lang_code: String,
    pub body: String,
    pub updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
pub(super) struct ProblemRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub languages: String,
    pub testdata_version: i32,
    pub testdata_sha256: Option<String>,
    pub default_lang_code: String,
    pub created_by: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub version: i64,
}

impl ProblemRow {
    pub fn response(self) -> Result<ProblemResponse, AppError> {
        let languages = serde_json::from_str(&self.languages)
            .map_err(|error| AppError::internal("decode problems.languages", error))?;
        Ok(ProblemResponse {
            id: self.id,
            slug: self.slug,
            title: self.title,
            time_limit_ms: self.time_limit_ms,
            memory_limit_mb: self.memory_limit_mb,
            output_limit_kb: self.output_limit_kb,
            languages,
            testdata_version: self.testdata_version,
            testdata_sha256: self.testdata_sha256,
            default_lang_code: self.default_lang_code,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
        })
    }
}

fn validate_slug(value: String) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    let valid = (1..=64).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.contains("--");
    if valid {
        Ok(value)
    } else {
        Err(AppError::validation("slug", "must be a lowercase kebab-case identifier"))
    }
}

fn validate_title(value: String) -> Result<String, AppError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > 255 {
        Err(AppError::validation("title", "must contain between 1 and 255 characters"))
    } else {
        Ok(value)
    }
}

fn validate_limit(
    field: &'static str,
    value: i32,
    minimum: i32,
    maximum: i32,
) -> Result<i32, AppError> {
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::validation(field, "is outside the supported range"))
    }
}

fn validate_languages(mut values: Vec<String>) -> Result<String, AppError> {
    values.sort();
    values.dedup();
    if values.is_empty()
        || values.len() > P0_LANGUAGES.len()
        || values.iter().any(|value| !P0_LANGUAGES.contains(&value.as_str()))
    {
        return Err(AppError::validation(
            "languages",
            "must contain one or more of c, cpp, java, or python",
        ));
    }
    serde_json::to_string(&values)
        .map_err(|error| AppError::internal("encode problem languages", error))
}

fn validate_lang_code(value: String) -> Result<String, AppError> {
    validate_lang_code_field("defaultLangCode", value)
}

pub(super) fn validate_lang_code_field(
    field: &'static str,
    value: String,
) -> Result<String, AppError> {
    let valid = matches!(value.len(), 2 | 5)
        && value.as_bytes()[0..2].iter().all(u8::is_ascii_lowercase)
        && (value.len() == 2
            || (value.as_bytes()[2] == b'-'
                && value.as_bytes()[3..5].iter().all(u8::is_ascii_uppercase)));
    if valid {
        Ok(value)
    } else {
        Err(AppError::validation(field, "must be a language tag such as en or zh-CN"))
    }
}

#[cfg(test)]
mod tests {
    use super::{CreateProblemRequest, UpdateProblemRequest, UpsertStatementRequest};

    #[test]
    fn create_normalizes_and_sorts_languages() {
        let value = CreateProblemRequest {
            slug: " Two-Sum ".into(),
            title: " Two Sum ".into(),
            time_limit_ms: None,
            memory_limit_mb: None,
            output_limit_kb: None,
            languages: Some(vec!["python".into(), "cpp".into(), "cpp".into()]),
            default_lang_code: Some("zh-CN".into()),
        }
        .validate()
        .expect("valid problem");
        assert_eq!(value.slug, "two-sum");
        assert_eq!(value.languages_json, r#"["cpp","python"]"#);
    }

    #[test]
    fn unsupported_language_and_empty_update_are_rejected() {
        let create = CreateProblemRequest {
            slug: "a".into(),
            title: "A".into(),
            time_limit_ms: None,
            memory_limit_mb: None,
            output_limit_kb: None,
            languages: Some(vec!["rust".into()]),
            default_lang_code: None,
        };
        assert!(create.validate().is_err());
        assert!(
            UpdateProblemRequest {
                expected_version: 0,
                slug: None,
                title: None,
                time_limit_ms: None,
                memory_limit_mb: None,
                output_limit_kb: None,
                languages: None,
                default_lang_code: None,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn statement_language_and_size_are_bounded() {
        assert!(
            UpsertStatementRequest { body: "# Statement".into() }.validate("zh-CN".into()).is_ok()
        );
        assert!(
            UpsertStatementRequest { body: "body".into() }.validate("../../en".into()).is_err()
        );
    }
}
