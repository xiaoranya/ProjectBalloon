use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_TIME_LIMIT_MS: i32 = 1_000;
const DEFAULT_MEMORY_LIMIT_MB: i32 = 256;
const DEFAULT_OUTPUT_LIMIT_KB: i32 = 65_536;
const DEFAULT_LANG_CODE: &str = "en";
const ALLOWED_LANGUAGES: [&str; 5] = ["c", "cpp", "java", "output", "python"];

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
    #[serde(default)]
    pub judge_mode: Option<String>,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
    pub judge_mode: Option<String>,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
    pub judge_mode: String,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
    pub judge_mode: Option<String>,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
            self.languages
                .unwrap_or_else(|| ["c", "cpp", "java", "python"].map(str::to_owned).to_vec()),
        )?;
        let default_lang_code =
            validate_lang_code(self.default_lang_code.unwrap_or_else(|| DEFAULT_LANG_CODE.into()))?;
        let (judge_mode, interactor_object_key, interactor_sha256) = validate_judge_mode(
            self.judge_mode.unwrap_or_else(|| "STANDARD".into()),
            self.interactor_object_key,
            self.interactor_sha256,
        )?;
        let configured_languages: Vec<String> = serde_json::from_str(&languages_json)
            .map_err(|error| AppError::internal("decode validated languages", error))?;
        validate_languages_for_judge_mode(&configured_languages, &judge_mode)?;
        Ok(ValidatedProblem {
            slug,
            title,
            time_limit_ms,
            memory_limit_mb,
            output_limit_kb,
            languages_json,
            default_lang_code,
            judge_mode,
            interactor_object_key,
            interactor_sha256,
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
            && self.judge_mode.is_none()
            && self.interactor_object_key.is_none()
            && self.interactor_sha256.is_none()
        {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        let (judge_mode, interactor_object_key, interactor_sha256) =
            if let Some(mode) = self.judge_mode {
                let (mode, key, hash) =
                    validate_judge_mode(mode, self.interactor_object_key, self.interactor_sha256)?;
                (Some(mode), key, hash)
            } else {
                if self.interactor_object_key.is_some() || self.interactor_sha256.is_some() {
                    return Err(AppError::validation(
                        "judgeMode",
                        "must be supplied when changing the interactor",
                    ));
                }
                (None, None, None)
            };
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
            judge_mode,
            interactor_object_key,
            interactor_sha256,
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
    pub judge_mode: String,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
// Used only by the utoipa OpenAPI macro; Rust cannot see that generated use.
#[allow(dead_code)]
pub struct AttachmentUploadRequest {
    #[schema(example = "SAMPLE")]
    pub kind: String,
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, ToSchema)]
#[schema(as = TestdataUploadRequest)]
// Used only by the utoipa OpenAPI macro; Rust cannot see that generated use.
#[allow(dead_code)]
pub struct TestdataUploadRequest {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, ToSchema)]
#[schema(as = InteractorUploadRequest)]
// Used only by the utoipa OpenAPI macro; Rust cannot see that generated use.
#[allow(dead_code)]
pub struct InteractorUploadRequest {
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
    pub judge_mode: String,
    pub interactor_object_key: Option<String>,
    pub interactor_sha256: Option<String>,
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
            judge_mode: self.judge_mode,
            interactor_object_key: self.interactor_object_key,
            interactor_sha256: self.interactor_sha256,
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

fn validate_judge_mode(
    mode: String,
    interactor_object_key: Option<String>,
    interactor_sha256: Option<String>,
) -> Result<(String, Option<String>, Option<String>), AppError> {
    let mode = mode.trim().to_ascii_uppercase();
    if !matches!(mode.as_str(), "STANDARD" | "INTERACTIVE" | "OUTPUT_ONLY") {
        return Err(AppError::validation(
            "judgeMode",
            "must be STANDARD, INTERACTIVE, or OUTPUT_ONLY",
        ));
    }
    let key = interactor_object_key
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let hash = interactor_sha256
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    if mode == "INTERACTIVE" {
        if key.as_ref().is_none_or(|value| value.len() > 512 || value.chars().any(char::is_control))
            || hash.as_ref().is_none_or(|value| {
                value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(AppError::validation(
                "interactor",
                "interactive problems require a safe object key and SHA-256",
            ));
        }
    } else if key.is_some() || hash.is_some() {
        return Err(AppError::validation(
            "interactor",
            "only INTERACTIVE problems may configure an interactor",
        ));
    }
    Ok((mode, key, hash))
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
        || values.len() > ALLOWED_LANGUAGES.len()
        || values.iter().any(|value| !ALLOWED_LANGUAGES.contains(&value.as_str()))
    {
        return Err(AppError::validation(
            "languages",
            "must contain one or more of c, cpp, java, output, or python",
        ));
    }
    serde_json::to_string(&values)
        .map_err(|error| AppError::internal("encode problem languages", error))
}

pub(super) fn validate_languages_for_judge_mode(
    languages: &[String],
    judge_mode: &str,
) -> Result<(), AppError> {
    let output_only = judge_mode == "OUTPUT_ONLY";
    let only_output = languages.len() == 1 && languages[0] == "output";
    if output_only != only_output
        || (!output_only && languages.iter().any(|language| language == "output"))
    {
        return Err(AppError::validation(
            "languages",
            "OUTPUT_ONLY requires only the output language, and other modes cannot use output",
        ));
    }
    Ok(())
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
    use crate::features::problems::model::{
        CreateProblemRequest, UpdateProblemRequest, UpsertStatementRequest,
    };

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
            judge_mode: None,
            interactor_object_key: None,
            interactor_sha256: None,
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
            judge_mode: None,
            interactor_object_key: None,
            interactor_sha256: None,
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
                judge_mode: None,
                interactor_object_key: None,
                interactor_sha256: None,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn output_language_is_exclusive_to_output_only_mode() {
        let mixed = CreateProblemRequest {
            slug: "mixed".into(),
            title: "Mixed".into(),
            time_limit_ms: None,
            memory_limit_mb: None,
            output_limit_kb: None,
            languages: Some(vec!["cpp".into(), "output".into()]),
            default_lang_code: None,
            judge_mode: None,
            interactor_object_key: None,
            interactor_sha256: None,
        };
        assert!(mixed.validate().is_err());

        let output_only = CreateProblemRequest {
            slug: "output".into(),
            title: "Output".into(),
            time_limit_ms: None,
            memory_limit_mb: None,
            output_limit_kb: None,
            languages: Some(vec!["output".into()]),
            default_lang_code: None,
            judge_mode: Some("OUTPUT_ONLY".into()),
            interactor_object_key: None,
            interactor_sha256: None,
        };
        assert!(output_only.validate().is_ok());
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
