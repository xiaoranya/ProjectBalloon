use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaylistItemRequest {
    pub target_view: String,
    pub duration_seconds: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaylistRequest {
    pub name: String,
    pub loop_enabled: bool,
    pub items: Vec<PlaylistItemRequest>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItemResponse {
    pub id: i64,
    pub target_view: String,
    pub duration_seconds: i32,
    pub display_order: i32,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistResponse {
    pub id: i64,
    pub contest_id: i64,
    pub name: String,
    pub loop_enabled: bool,
    pub version: i64,
    #[sqlx(skip)]
    pub items: Vec<PlaylistItemResponse>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupRequest {
    pub name: String,
    pub instance_ids: Vec<i64>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupControlRequest {
    pub action: String,
    pub playlist_id: Option<i64>,
    pub target_view: Option<String>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct GroupResponse {
    pub id: i64,
    pub contest_id: i64,
    pub name: String,
    #[sqlx(skip)]
    pub instance_ids: Vec<i64>,
    pub playlist_id: Option<i64>,
    pub playback_status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub playback_started_at: Option<OffsetDateTime>,
    pub paused_elapsed_seconds: i64,
    pub locked_view: Option<String>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupPlaybackResponse {
    pub group_id: i64,
    pub group_name: String,
    pub playlist_id: Option<i64>,
    pub loop_enabled: bool,
    pub status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    pub paused_elapsed_seconds: i64,
    pub locked_view: Option<String>,
    pub version: i64,
    pub items: Vec<PlaylistItemResponse>,
}
