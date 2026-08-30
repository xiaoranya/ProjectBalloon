use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::features::problems::render_safe_statement;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BankQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_size")]
    pub size: u32,
    pub tag: Option<String>,
    pub difficulty: Option<i16>,
}
const fn default_size() -> u32 {
    50
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BankProblem {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub statement: Option<String>,
    pub difficulty: Option<i16>,
    pub tags: serde_json::Value,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub published_at: Option<time::OffsetDateTime>,
    pub languages: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemPublication {
    pub visibility: String,
    pub difficulty: Option<i16>,
    pub tags: Vec<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub published_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, FromRow)]
pub(super) struct BankProblemRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub statement: Option<String>,
    pub difficulty: Option<i16>,
    pub tags: serde_json::Value,
    pub published_at: Option<time::OffsetDateTime>,
    pub languages: String,
}

impl TryFrom<BankProblemRow> for BankProblem {
    type Error = AppError;

    fn try_from(row: BankProblemRow) -> Result<Self, Self::Error> {
        let languages = serde_json::from_str(&row.languages)
            .map_err(|error| AppError::internal("decode bank problem languages", error))?;
        Ok(BankProblem {
            id: row.id,
            slug: row.slug,
            title: row.title,
            statement: row.statement.map(|s| render_safe_statement(&s)),
            difficulty: row.difficulty,
            tags: row.tags,
            published_at: row.published_at,
            languages,
        })
    }
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSet {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub visibility: String,
    pub item_count: i64,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TrainingItem {
    pub problem_id: i64,
    pub slug: String,
    pub title: String,
    pub position: i32,
    pub required: bool,
    pub difficulty: Option<i16>,
    pub tags: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrainingSetDetail {
    pub set_info: TrainingSet,
    pub items: Vec<TrainingItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetRequest {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub items: Vec<SetItemRequest>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetItemRequest {
    pub problem_id: i64,
    pub required: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicationRequest {
    pub visibility: String,
    pub difficulty: Option<i16>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressRequest {
    pub problem_id: i64,
    pub status: String,
    pub score: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FavoriteRequest {
    pub favorite: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteResponse {
    pub(super) problem_id: i64,
    pub(super) favorite: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorialRequest {
    pub title: String,
    pub body: String,
    pub unlock_policy: String,
    pub published: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EditorialResponse {
    pub(super) problem_id: i64,
    pub(super) lang_code: String,
    pub(super) title: String,
    pub(super) body_html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) body_markdown: Option<String>,
    pub(super) unlock_policy: String,
    pub(super) unlocked: bool,
    pub(super) updated_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSettingsResponse {
    daily_submission_limit: i32,
    concurrent_judging_limit: i32,
    source_retention_days: i32,
    updated_at: time::OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PracticeSettingsRequest {
    pub(super) daily_submission_limit: i32,
    pub(super) concurrent_judging_limit: i32,
    pub(super) source_retention_days: i32,
}

#[derive(Debug, Serialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Enrollment {
    pub id: i64,
    pub set_id: i64,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub status: String,
    pub started_at: time::OffsetDateTime,
    pub completed_at: Option<time::OffsetDateTime>,
}

pub(super) fn validate_page(query: &BankQuery) -> Result<(i64, i64), AppError> {
    if !(1..=100).contains(&query.size) {
        return Err(AppError::validation("size", "must be between 1 and 100"));
    }
    let offset = i64::from(query.page)
        .checked_mul(i64::from(query.size))
        .ok_or_else(|| AppError::validation("page", "is too large"))?;
    Ok((i64::from(query.size), offset))
}
