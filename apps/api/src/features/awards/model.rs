use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryRequest {
    pub code: String,
    pub name: String,
    pub display_order: i32,
    #[serde(default)]
    pub include_star: bool,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    #[serde(default)]
    pub first_blood: bool,
    pub rule: RuleRequest,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleRequest {
    pub rule_type: String,
    pub ratio: Option<f64>,
    pub fixed_count: Option<i32>,
    pub rank_from: Option<i32>,
    pub rank_to: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateRequest {
    pub resolver_run_id: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRequest {
    pub expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualRecipientRequest {
    pub category_id: i64,
    pub team_id: i64,
    pub expected_set_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCategoryRequest {
    pub expected_version: i32,
    #[serde(flatten)]
    pub category: CategoryRequest,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResponse {
    pub id: i64,
    pub contest_id: i64,
    pub code: String,
    pub name: String,
    pub display_order: i32,
    pub include_star: bool,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    pub first_blood: bool,
    pub version: i32,
    pub rule_type: String,
    pub ratio: Option<f64>,
    pub fixed_count: Option<i32>,
    pub rank_from: Option<i32>,
    pub rank_to: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecipientResponse {
    pub id: i64,
    pub category_id: i64,
    pub category_code: String,
    pub category_name: String,
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub rank: Option<i32>,
    pub solved: Option<i32>,
    pub penalty_minutes: Option<i64>,
    pub participation_type: Option<String>,
    pub group_name: Option<String>,
    pub is_star: bool,
    pub is_manual: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardCandidateResponse {
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub rank: u32,
    pub participation_type: String,
    pub group_name: Option<String>,
    pub is_star: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AwardResolverRunResponse {
    pub id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub completed_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardSetResponse {
    pub id: i64,
    pub contest_id: i64,
    pub resolver_run_id: i64,
    pub final_scoreboard_snapshot_id: i64,
    pub status: String,
    pub version: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub frozen_at: Option<OffsetDateTime>,
    pub recipients: Vec<RecipientResponse>,
    pub conflicts: Vec<AwardConflict>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardConflict {
    pub team_id: i64,
    pub team_name: String,
    pub category_codes: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationRequest {
    pub current_category_id: Option<i64>,
    pub status: String,
    #[serde(default)]
    pub auto_rotate: bool,
    pub interval_seconds: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationRecipient {
    pub id: i64,
    pub problem_id: Option<i64>,
    pub problem_alias: Option<String>,
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    pub star: bool,
    pub rank: Option<i32>,
    pub solved: Option<i32>,
    pub penalty_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCategory {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub display_order: i32,
    pub group_name: Option<String>,
    pub first_blood: bool,
    #[sqlx(skip)]
    pub recipients: Vec<PresentationRecipient>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresentationResponse {
    pub contest_id: i64,
    pub contest_name: String,
    pub contest_status: String,
    #[serde(with = "time::serde::rfc3339")]
    pub server_time: OffsetDateTime,
    pub status: String,
    pub current_category_id: i64,
    pub auto_rotate: bool,
    pub interval_seconds: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub state_updated_at: OffsetDateTime,
    pub categories: Vec<PresentationCategory>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostScriptSectionRequest {
    pub category_id: i64,
    pub cue_text: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostScriptRequest {
    pub opening_text: String,
    pub closing_text: String,
    pub sections: Vec<HostScriptSectionRequest>,
    pub expected_version: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostScriptSectionResponse {
    pub category_id: i64,
    pub code: String,
    pub name: String,
    pub first_blood: bool,
    pub current: bool,
    pub cue_text: String,
    pub recipients: Vec<PresentationRecipient>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostScriptResponse {
    pub contest_id: i64,
    pub contest_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub server_time: OffsetDateTime,
    pub presentation_status: String,
    pub current_category_id: i64,
    pub next_category_id: Option<i64>,
    pub auto_rotate: bool,
    pub interval_seconds: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub state_updated_at: OffsetDateTime,
    pub version: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub opening_text: String,
    pub closing_text: String,
    pub sections: Vec<HostScriptSectionResponse>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CertificateRow {
    pub certificate_no: String,
    pub contest_id: i64,
    pub contest_name: String,
    pub award_code: String,
    pub award_name: String,
    pub problem_alias: Option<String>,
    pub team_id: i64,
    pub team_name: String,
    pub school: Option<String>,
    pub source_member_id: Option<i64>,
    pub recipient_name: String,
    pub recipient_role: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub participation_type: Option<String>,
    pub rank: Option<i32>,
}
