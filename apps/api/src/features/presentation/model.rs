use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::features::presentation::orchestration::GroupPlaybackResponse;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigRequest {
    pub(super) enabled: bool,
    pub(super) title: Option<String>,
    pub(super) subtitle: Option<String>,
    pub(super) accent_color: String,
    pub(super) row_limit: i32,
    pub(super) show_announcements: bool,
    pub(super) announcement_interval_seconds: i32,
    #[serde(default)]
    pub(super) template: Option<String>,
    #[serde(default)]
    pub(super) custom_template_id: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub(super) contest_id: i64,
    pub(super) mode: String,
    pub(super) enabled: bool,
    pub(super) title: Option<String>,
    pub(super) subtitle: Option<String>,
    pub(super) accent_color: String,
    pub(super) row_limit: i32,
    pub(super) show_announcements: bool,
    pub(super) announcement_interval_seconds: i32,
    pub(super) template: String,
    pub(super) custom_template_id: Option<i64>,
    pub(super) custom_template_name: Option<String>,
    pub(super) custom_background_color: Option<String>,
    pub(super) custom_foreground_color: Option<String>,
    pub(super) custom_accent_color: Option<String>,
    pub(super) custom_font_family: Option<String>,
    pub(super) custom_density: Option<String>,
    pub(super) custom_show_clock: Option<bool>,
    pub(super) custom_show_logo: Option<bool>,
    pub(super) custom_logo_object_key: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterRequest {
    pub(super) contest_id: i64,
    pub(super) name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationResponse {
    pub(super) instance_id: i64,
    pub(super) contest_id: i64,
    pub(super) name: String,
    pub(super) client_token: String,
    pub(super) current_view: String,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) registered_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeartbeatRequest {
    pub(super) client_token: String,
    pub(super) current_view: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    pub(super) instance_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) server_time: OffsetDateTime,
    pub(super) command_id: Option<i64>,
    pub(super) target_view: Option<String>,
    pub(super) group_playback: Option<GroupPlaybackResponse>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct InstanceResponse {
    pub(super) id: i64,
    pub(super) contest_id: i64,
    pub(super) name: String,
    pub(super) current_view: String,
    pub(super) online: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) last_seen_at: Option<OffsetDateTime>,
    pub(super) last_ip: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) revoked_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    pub(super) target_view: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    pub(super) id: i64,
    pub(super) screen_instance_id: i64,
    pub(super) target_view: String,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModeQuery {
    pub(super) mode: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplateResponse {
    pub(super) id: i64,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) background_color: String,
    pub(super) foreground_color: String,
    pub(super) accent_color: String,
    pub(super) font_family: String,
    pub(super) density: String,
    pub(super) show_clock: bool,
    pub(super) show_logo: bool,
    pub(super) logo_object_key: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationTemplateRequest {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: String,
    pub(super) background_color: String,
    pub(super) foreground_color: String,
    pub(super) accent_color: String,
    #[serde(default = "default_font")]
    pub(super) font_family: String,
    #[serde(default = "default_density")]
    pub(super) density: String,
    #[serde(default = "default_true")]
    pub(super) show_clock: bool,
    #[serde(default)]
    pub(super) show_logo: bool,
    pub(super) logo_object_key: Option<String>,
}

fn default_font() -> String {
    "Inter".to_owned()
}
fn default_density() -> String {
    "COMFORTABLE".to_owned()
}
const fn default_true() -> bool {
    true
}
