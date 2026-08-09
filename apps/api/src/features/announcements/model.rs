use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    pub(crate) title: String,
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) pinned: bool,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub(crate) scheduled_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRequest {
    pub(crate) title: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) pinned: Option<bool>,
    pub(crate) expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinRequest {
    pub(crate) pinned: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListQuery {
    #[serde(default)]
    pub(crate) include_withdrawn: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementResponse {
    pub id: i64,
    pub contest_id: i64,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub status: String,
    pub created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub scheduled_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub withdrawn_at: Option<OffsetDateTime>,
    pub withdrawn_by_user_id: Option<i64>,
    pub source_clarification_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub cancelled_at: Option<OffsetDateTime>,
    pub cancelled_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub version: i32,
}
