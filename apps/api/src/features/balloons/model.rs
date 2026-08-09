use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub(super) status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchQuery {
    pub limit: Option<i32>,
    pub zone: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DispatchPolicyResponse {
    pub(super) contest_id: i64,
    pub(super) strategy: String,
    pub(super) max_batch: i32,
    pub(super) cooldown_seconds: i32,
    pub(super) zone_order: serde_json::Value,
    pub(super) updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchPolicyRequest {
    pub strategy: String,
    pub max_batch: i32,
    pub cooldown_seconds: i32,
    pub zone_order: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRequest {
    pub(super) expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelRequest {
    pub(super) expected_version: i32,
    pub(super) reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteRequest {
    pub(super) expected_version: i32,
    pub(super) note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BalloonTaskResponse {
    pub(super) id: i64,
    pub(super) contest_id: i64,
    pub(super) team_id: i64,
    pub(super) problem_id: i64,
    pub(super) submission_id: i64,
    pub(super) color: String,
    pub(super) is_first_blood: bool,
    pub(super) status: String,
    pub(super) seat_no: Option<String>,
    pub(super) team_name: String,
    pub(super) problem_alias: String,
    pub(super) note: Option<String>,
    pub(super) claimed_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) claimed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) delivered_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) cancelled_at: Option<OffsetDateTime>,
    pub(super) cancelled_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) updated_at: OffsetDateTime,
    pub(super) version: i32,
    pub(super) reopened_count: i32,
    pub(super) priority: i32,
    pub(super) delivery_zone: String,
    pub(super) dispatch_attempts: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) last_dispatched_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BalloonStatsResponse {
    pub(super) total: i64,
    pub(super) pending: i64,
    pub(super) claimed: i64,
    pub(super) delivered: i64,
    pub(super) cancelled: i64,
    pub(super) first_blood: i64,
}
