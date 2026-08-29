use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::error::AppError;

use crate::features::problems::render_safe_statement;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ContestProblemListQuery {
    pub lang: Option<String>,
}

impl ContestProblemListQuery {
    pub fn validate(self) -> Result<Option<String>, AppError> {
        self.lang.map(validate_lang_code).transpose()
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssignProblemRequest {
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContestProblemRequest {
    pub alias: Option<String>,
    pub display_order: Option<i32>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReorderEntry {
    pub problem_id: i64,
    pub display_order: i32,
}

pub struct ValidatedReorderEntry {
    pub problem_id: i64,
    pub display_order: i32,
}

pub fn validate_reorder(
    entries: Vec<ReorderEntry>,
) -> Result<Vec<ValidatedReorderEntry>, AppError> {
    if entries.is_empty() || entries.len() > 1_000 {
        return Err(AppError::validation(
            "request",
            "must contain between 1 and 1000 reorder entries",
        ));
    }
    let mut problem_ids = HashSet::with_capacity(entries.len());
    let mut orders = HashSet::with_capacity(entries.len());
    entries
        .into_iter()
        .map(|entry| {
            if entry.problem_id <= 0 {
                return Err(AppError::validation("problemId", "must be positive"));
            }
            let display_order = validate_order(entry.display_order)?;
            if !problem_ids.insert(entry.problem_id) {
                return Err(AppError::validation("problemId", "must not contain duplicates"));
            }
            if !orders.insert(display_order) {
                return Err(AppError::validation("displayOrder", "must not contain duplicates"));
            }
            Ok(ValidatedReorderEntry { problem_id: entry.problem_id, display_order })
        })
        .collect()
}

pub struct ValidatedAssignment {
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    pub color: Option<String>,
}

pub struct ValidatedAssignmentUpdate {
    pub alias: Option<String>,
    pub display_order: Option<i32>,
    pub color: Option<String>,
}

impl AssignProblemRequest {
    pub fn validate(self) -> Result<ValidatedAssignment, AppError> {
        if self.problem_id <= 0 {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        Ok(ValidatedAssignment {
            problem_id: self.problem_id,
            alias: validate_alias(self.alias)?,
            display_order: validate_order(self.display_order)?,
            color: self.color.map(validate_color).transpose()?,
        })
    }
}

impl UpdateContestProblemRequest {
    pub fn validate(self) -> Result<ValidatedAssignmentUpdate, AppError> {
        if self.alias.is_none() && self.display_order.is_none() && self.color.is_none() {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        Ok(ValidatedAssignmentUpdate {
            alias: self.alias.map(validate_alias).transpose()?,
            display_order: self.display_order.map(validate_order).transpose()?,
            color: self.color.map(validate_color).transpose()?,
        })
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ContestProblemResponse {
    pub contest_id: i64,
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    pub color: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestProblemDetailResponse {
    pub contest_id: i64,
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    pub color: Option<String>,
    pub slug: String,
    pub title: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub languages: Vec<String>,
    pub statement: Option<PublishedStatementResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishedStatementResponse {
    pub lang_code: String,
    pub rendered_html: String,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
pub(super) struct ContestProblemDetailRow {
    pub contest_id: i64,
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    pub color: Option<String>,
    pub slug: String,
    pub title: String,
    pub time_limit_ms: i32,
    pub memory_limit_mb: i32,
    pub output_limit_kb: i32,
    pub languages: String,
    pub statement_lang_code: Option<String>,
    pub statement_body: Option<String>,
    pub statement_updated_at: Option<OffsetDateTime>,
}

impl ContestProblemDetailRow {
    pub fn response(self) -> Result<ContestProblemDetailResponse, AppError> {
        let languages = serde_json::from_str(&self.languages)
            .map_err(|error| AppError::internal("decode contest problem languages", error))?;
        let statement =
            match (self.statement_lang_code, self.statement_body, self.statement_updated_at) {
                (Some(lang_code), Some(body), Some(updated_at)) => {
                    Some(PublishedStatementResponse {
                        lang_code,
                        rendered_html: render_safe_statement(&body),
                        updated_at,
                    })
                }
                (None, None, None) => None,
                _ => {
                    return Err(AppError::internal_message(
                        "decode contest problem statement",
                        "inconsistent nullable statement columns",
                    ));
                }
            };
        Ok(ContestProblemDetailResponse {
            contest_id: self.contest_id,
            problem_id: self.problem_id,
            alias: self.alias,
            display_order: self.display_order,
            color: self.color,
            slug: self.slug,
            title: self.title,
            time_limit_ms: self.time_limit_ms,
            memory_limit_mb: self.memory_limit_mb,
            output_limit_kb: self.output_limit_kb,
            languages,
            statement,
        })
    }
}

fn validate_alias(value: String) -> Result<String, AppError> {
    let value = value.trim().to_ascii_uppercase();
    if (1..=8).contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        Ok(value)
    } else {
        Err(AppError::validation("alias", "must contain 1 to 8 uppercase ASCII letters or digits"))
    }
}

fn validate_order(value: i32) -> Result<i32, AppError> {
    if (1..=1_000).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::validation("displayOrder", "must contain a value between 1 and 1000"))
    }
}

fn validate_color(value: String) -> Result<String, AppError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > 16 {
        Err(AppError::validation("color", "must contain between 1 and 16 characters"))
    } else {
        Ok(value)
    }
}

fn validate_lang_code(value: String) -> Result<String, AppError> {
    let valid = matches!(value.len(), 2 | 5)
        && value.as_bytes()[0..2].iter().all(u8::is_ascii_lowercase)
        && (value.len() == 2
            || (value.as_bytes()[2] == b'-'
                && value.as_bytes()[3..5].iter().all(u8::is_ascii_uppercase)));
    if valid {
        Ok(value)
    } else {
        Err(AppError::validation("lang", "must be a language tag such as en or zh-CN"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssignProblemRequest, ReorderEntry, UpdateContestProblemRequest, validate_reorder,
    };

    #[test]
    fn assignment_is_normalized() {
        let value = AssignProblemRequest {
            problem_id: 1,
            alias: " a1 ".into(),
            display_order: 1,
            color: Some(" red ".into()),
        }
        .validate()
        .expect("valid assignment");
        assert_eq!(value.alias, "A1");
        assert_eq!(value.color.as_deref(), Some("red"));
    }

    #[test]
    fn invalid_or_empty_changes_are_rejected() {
        assert!(
            AssignProblemRequest {
                problem_id: 1,
                alias: "A-1".into(),
                display_order: 1,
                color: None,
            }
            .validate()
            .is_err()
        );
        assert!(
            UpdateContestProblemRequest { alias: None, display_order: None, color: None }
                .validate()
                .is_err()
        );
    }

    #[test]
    fn reorder_rejects_duplicate_ids_and_orders() {
        assert!(
            validate_reorder(vec![
                ReorderEntry { problem_id: 1, display_order: 1 },
                ReorderEntry { problem_id: 1, display_order: 2 },
            ])
            .is_err()
        );
        assert!(
            validate_reorder(vec![
                ReorderEntry { problem_id: 1, display_order: 1 },
                ReorderEntry { problem_id: 2, display_order: 1 },
            ])
            .is_err()
        );
    }
}
