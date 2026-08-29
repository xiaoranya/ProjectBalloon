use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardQuery {
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
}

#[derive(Clone)]
pub struct ValidatedScoreboardQuery {
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSelector {
    pub variant: String,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
}

pub struct ValidatedSnapshotSelector {
    pub variant: &'static str,
    pub query: ValidatedScoreboardQuery,
}

impl SnapshotSelector {
    pub fn validate(self) -> Result<ValidatedSnapshotSelector, AppError> {
        let variant = match self.variant.to_ascii_uppercase().as_str() {
            "PUBLIC" => "PUBLIC",
            "ADMIN" => "ADMIN",
            _ => return Err(AppError::validation("variant", "must be PUBLIC or ADMIN")),
        };
        let query = ScoreboardQuery {
            group_name: self.group_name,
            participation_type: self.participation_type,
        }
        .validate()?;
        Ok(ValidatedSnapshotSelector { variant, query })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardSnapshotResponse {
    pub id: i64,
    pub contest_id: i64,
    pub variant: String,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    pub version: i64,
    pub frozen: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    pub payload_sha256: String,
    pub payload: Value,
}

#[derive(sqlx::FromRow)]
pub(super) struct SnapshotRow {
    pub id: i64,
    pub contest_id: i64,
    pub variant: String,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    pub version: i64,
    pub frozen: bool,
    pub generated_at: OffsetDateTime,
    pub payload_json: String,
    pub payload_sha256: Option<String>,
}

impl SnapshotRow {
    pub fn response(self) -> Result<ScoreboardSnapshotResponse, AppError> {
        let payload = serde_json::from_str(&self.payload_json)
            .map_err(|error| AppError::internal("decode scoreboard snapshot payload", error))?;
        Ok(ScoreboardSnapshotResponse {
            id: self.id,
            contest_id: self.contest_id,
            variant: self.variant,
            group_name: self.group_name,
            participation_type: self.participation_type,
            version: self.version,
            frozen: self.frozen,
            generated_at: self.generated_at,
            payload_sha256: self.payload_sha256.ok_or_else(|| {
                AppError::internal_message(
                    "load scoreboard snapshot",
                    "snapshot has no payload SHA-256",
                )
            })?,
            payload,
        })
    }
}

impl ScoreboardQuery {
    pub fn validate(self) -> Result<ValidatedScoreboardQuery, AppError> {
        let group_name =
            self.group_name.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
        if group_name.as_ref().is_some_and(|value| value.chars().count() > 128) {
            return Err(AppError::validation("groupName", "must contain at most 128 characters"));
        }
        let participation_type = self.participation_type.map(|value| value.to_ascii_uppercase());
        if participation_type
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "OFFICIAL" | "STAR" | "PRACTICE"))
        {
            return Err(AppError::validation(
                "participationType",
                "must be OFFICIAL, STAR, or PRACTICE",
            ));
        }
        Ok(ValidatedScoreboardQuery { group_name, participation_type })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardResponse {
    pub contest_id: i64,
    pub variant: String,
    pub frozen: bool,
    #[serde(default = "default_scoring_mode")]
    pub scoring_mode: String,
    #[serde(default = "default_score_aggregation")]
    pub score_aggregation: String,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    pub problems: Vec<ScoreboardProblem>,
    pub rows: Vec<ScoreboardRow>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardProblem {
    pub problem_id: i64,
    pub alias: String,
    pub display_order: i32,
    #[sqlx(default)]
    pub first_blood_team_id: Option<i64>,
    #[sqlx(default)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub first_blood_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardRow {
    pub rank: u32,
    pub official_rank: Option<u32>,
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub participation_type: String,
    pub group_name: Option<String>,
    pub is_star: bool,
    pub solved_count: i32,
    pub penalty_minutes: i64,
    #[serde(default)]
    pub total_score_milli: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_solved_at: Option<OffsetDateTime>,
    pub problems: Vec<ScoreboardCell>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoreboardCell {
    pub problem_id: i64,
    pub wrong_attempts: i32,
    pub solved: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub solved_at: Option<OffsetDateTime>,
    pub penalty_minutes: i64,
    #[serde(default)]
    pub score_milli: i32,
    pub first_blood: bool,
}

#[derive(sqlx::FromRow)]
pub(super) struct ContestBoardRow {
    pub status: String,
    pub start_at: Option<OffsetDateTime>,
    pub freeze_at: Option<OffsetDateTime>,
    pub end_at: Option<OffsetDateTime>,
    pub scoreboard_revision: i64,
    pub scoring_mode: String,
    pub score_aggregation: String,
}

#[derive(sqlx::FromRow)]
pub(super) struct RosterRow {
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub participation_type: String,
    pub group_name: Option<String>,
    pub team_star: bool,
}

#[derive(sqlx::FromRow)]
pub(super) struct CellRow {
    pub team_id: i64,
    pub problem_id: i64,
    pub wrong_attempts: i32,
    pub solved: bool,
    pub solved_at: Option<OffsetDateTime>,
    pub penalty_minutes: i64,
    pub score_milli: i32,
}

#[derive(sqlx::FromRow)]
pub(super) struct SubmissionScoreRow {
    pub submission_id: i64,
    pub team_id: i64,
    pub problem_id: i64,
    pub submitted_at: OffsetDateTime,
    pub verdict: String,
    pub score_milli: i32,
    pub max_score_milli: i32,
}

fn default_scoring_mode() -> String {
    "ICPC".to_owned()
}

fn default_score_aggregation() -> String {
    "BEST".to_owned()
}
