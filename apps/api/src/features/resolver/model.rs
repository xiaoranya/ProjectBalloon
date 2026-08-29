use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::features::scoreboard::{ScoreboardCell, ScoreboardResponse};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    pub(crate) public_snapshot_id: i64,
    pub(crate) final_snapshot_id: i64,
    #[serde(default)]
    pub(crate) official: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    pub(crate) expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoPlayRequest {
    pub(crate) expected_version: i32,
    pub(crate) enabled: bool,
    pub(crate) interval_milliseconds: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Reveal {
    pub(crate) team_id: i64,
    pub(crate) problem_id: i64,
    pub(crate) before: ScoreboardCell,
    pub(crate) after: ScoreboardCell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolverState {
    pub(crate) step_index: i32,
    pub(crate) total_steps: i32,
    pub(crate) board: ScoreboardResponse,
    pub(crate) last_reveal: Option<Reveal>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverRunResponse {
    pub(crate) id: i64,
    pub(crate) contest_id: i64,
    pub(crate) official: bool,
    pub(crate) status: String,
    pub(crate) current_step: i32,
    pub(crate) total_steps: i32,
    pub(crate) source_public_snapshot_id: i64,
    pub(crate) source_final_snapshot_id: i64,
    pub(crate) plan_sha256: String,
    pub(crate) created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(crate) started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(crate) completed_at: Option<OffsetDateTime>,
    pub(crate) auto_play_enabled: bool,
    pub(crate) auto_play_interval_milliseconds: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(crate) next_auto_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) updated_at: OffsetDateTime,
    pub(crate) version: i32,
    pub(crate) state: Value,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolverPublicStateResponse {
    pub(crate) id: i64,
    pub(crate) contest_id: i64,
    pub(crate) status: String,
    pub(crate) current_step: i32,
    pub(crate) total_steps: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) updated_at: OffsetDateTime,
    pub(crate) state: Value,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverEventResponse {
    pub(crate) id: i64,
    pub(crate) event_type: String,
    pub(crate) payload: Value,
    pub(crate) sequence: i32,
    pub(crate) actor_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSourceSnapshotResponse {
    pub(crate) id: i64,
    version: i64,
    #[serde(with = "time::serde::rfc3339")]
    generated_at: OffsetDateTime,
    payload_sha256: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolverSourcesResponse {
    pub(crate) public_snapshot: ResolverSourceSnapshotResponse,
    pub(crate) final_snapshot: ResolverSourceSnapshotResponse,
}

#[derive(sqlx::FromRow)]
pub(crate) struct RunRow {
    id: i64,
    contest_id: i64,
    official: bool,
    status: String,
    current_step: i32,
    total_steps: i32,
    source_public_snapshot_id: Option<i64>,
    source_final_snapshot_id: Option<i64>,
    plan_sha256: String,
    created_by_user_id: Option<i64>,
    started_at: Option<OffsetDateTime>,
    completed_at: Option<OffsetDateTime>,
    auto_play_enabled: bool,
    auto_play_interval_ms: i32,
    next_auto_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    version: i32,
    state_data: String,
}

impl RunRow {
    pub(crate) fn response(self) -> Result<ResolverRunResponse, AppError> {
        Ok(ResolverRunResponse {
            id: self.id,
            contest_id: self.contest_id,
            official: self.official,
            status: self.status,
            current_step: self.current_step,
            total_steps: self.total_steps,
            source_public_snapshot_id: self.source_public_snapshot_id.ok_or_else(|| {
                AppError::internal_message("load resolver run", "run has no public source snapshot")
            })?,
            source_final_snapshot_id: self.source_final_snapshot_id.ok_or_else(|| {
                AppError::internal_message("load resolver run", "run has no final source snapshot")
            })?,
            plan_sha256: self.plan_sha256,
            created_by_user_id: self.created_by_user_id.ok_or_else(|| {
                AppError::internal_message("load resolver run", "run has no creator")
            })?,
            started_at: self.started_at,
            completed_at: self.completed_at,
            auto_play_enabled: self.auto_play_enabled,
            auto_play_interval_milliseconds: self.auto_play_interval_ms,
            next_auto_at: self.next_auto_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            version: self.version,
            state: serde_json::from_str(&self.state_data)
                .map_err(|error| AppError::internal("decode resolver state", error))?,
        })
    }
}
