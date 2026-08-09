use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentInfoResponse {
    pub mode: &'static str,
    pub active_contest: Option<ActiveContestResponse>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveContestResponse {
    pub id: i64,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub start_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkstationLoginRequest {
    #[schema(min_length = 6, max_length = 32)]
    pub pairing_code: String,
}

impl WorkstationLoginRequest {
    pub fn validate(self) -> Result<String, crate::error::AppError> {
        let code = normalize_pairing_code(&self.pairing_code);
        if !(6..=32).contains(&code.len()) || !code.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err(crate::error::AppError::validation(
                "pairingCode",
                "must contain 6 to 32 letters or digits",
            ));
        }
        Ok(code)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkstationRequest {
    #[schema(min_length = 2, max_length = 45)]
    pub ip_address: String,
    #[schema(min_length = 1, max_length = 64)]
    pub seat_no: String,
    #[schema(max_length = 128)]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateWorkstationRequest {
    #[schema(min_length = 2, max_length = 45)]
    pub ip_address: String,
    #[schema(min_length = 1, max_length = 64)]
    pub seat_no: String,
    #[schema(max_length = 128)]
    pub label: Option<String>,
    pub enabled: bool,
    pub expected_version: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkstationResponse {
    pub id: i64,
    pub ip_address: String,
    pub seat_no: String,
    pub label: Option<String>,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<OffsetDateTime>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindWorkstationRequest {
    pub workstation_id: i64,
    pub team_id: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkstationBindingResponse {
    pub id: i64,
    pub contest_id: i64,
    pub workstation_id: i64,
    pub ip_address: String,
    pub seat_no: String,
    pub team_id: i64,
    pub team_name: String,
    pub pairing_code: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub bound_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionSessionResponse {
    pub contest_id: i64,
    pub contest_name: String,
    pub workstation_id: i64,
    pub seat_no: String,
}

#[derive(Debug, Clone)]
pub struct WorkstationLoginGrant {
    pub binding_id: i64,
    pub user_id: i64,
    pub bound_ip: String,
    pub competition: CompetitionSessionResponse,
    pub expires_at: OffsetDateTime,
}

pub fn normalize_pairing_code(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}
