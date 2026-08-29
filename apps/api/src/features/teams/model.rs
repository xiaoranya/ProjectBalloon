use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_PAGE_SIZE: u32 = 100;
const MAX_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ParticipationType {
    Official,
    Star,
    Practice,
}

impl ParticipationType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Official => "OFFICIAL",
            Self::Star => "STAR",
            Self::Practice => "PRACTICE",
        }
    }
}

impl std::str::FromStr for ParticipationType {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "OFFICIAL" => Ok(Self::Official),
            "STAR" => Ok(Self::Star),
            "PRACTICE" => Ok(Self::Practice),
            invalid => {
                Err(AppError::internal_message("invalid contest_teams.participation_type", invalid))
            }
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TeamListQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub sort: Option<String>,
    #[serde(default, rename = "includeDeleted")]
    pub include_deleted: bool,
}

const fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

pub struct ValidatedTeamListQuery {
    pub page: u32,
    pub size: u32,
    pub offset: i64,
    pub include_deleted: bool,
    pub order_by: &'static str,
}

impl TeamListQuery {
    pub fn validate(self) -> Result<ValidatedTeamListQuery, AppError> {
        if !(1..=MAX_PAGE_SIZE).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 500"));
        }
        let order_by = match self.sort.as_deref().unwrap_or("name,asc") {
            "name,asc" => "t.name ASC, t.id ASC",
            "name,desc" => "t.name DESC, t.id DESC",
            "createdAt,asc" => "t.created_at ASC, t.id ASC",
            "createdAt,desc" => "t.created_at DESC, t.id DESC",
            "updatedAt,asc" => "t.updated_at ASC, t.id ASC",
            "updatedAt,desc" => "t.updated_at DESC, t.id DESC",
            _ => return Err(AppError::validation("sort", "must use an allowed team sort")),
        };
        Ok(ValidatedTeamListQuery {
            page: self.page,
            size: self.size,
            offset: checked_offset(self.page, self.size)?,
            include_deleted: self.include_deleted,
            order_by,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTeamRequest {
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    #[serde(default)]
    pub star: bool,
    #[schema(min_length = 3, max_length = 64)]
    pub username: Option<String>,
    #[schema(min_length = 8, max_length = 128, write_only)]
    pub initial_password: Option<String>,
    /// Require the generated account to change its password at first login.
    /// Batch import ignores this row-level value and applies the batch-level
    /// `requirePasswordReset` field instead.
    #[serde(default = "default_require_password_reset")]
    pub require_password_reset: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedCreateTeam {
    pub name: String,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub star: bool,
    pub account: Option<ValidatedTeamAccount>,
    pub require_password_reset: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedTeamAccount {
    pub username: String,
    pub initial_password: String,
}

impl CreateTeamRequest {
    pub fn validate(self) -> Result<ValidatedCreateTeam, AppError> {
        let name = required_text("name", self.name, 255)?;
        let school = optional_text("school", self.school, 255)?;
        let seat_no = optional_text("seatNo", self.seat_no, 64)?;
        let group_name = optional_text("groupName", self.group_name, 128)?;
        let account = match (self.username, self.initial_password) {
            (None, None) => None,
            (Some(username), Some(initial_password)) => {
                let username = validate_username(username)?;
                validate_password("initialPassword", &initial_password)?;
                Some(ValidatedTeamAccount { username, initial_password })
            }
            _ => {
                return Err(AppError::validation(
                    "account",
                    "username and initialPassword must be provided together",
                ));
            }
        };
        Ok(ValidatedCreateTeam {
            name,
            school,
            seat_no,
            group_name,
            star: self.star,
            account,
            require_password_reset: self.require_password_reset,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTeamRequest {
    pub name: Option<String>,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub star: Option<bool>,
    pub expected_version: Option<i64>,
}

pub struct ValidatedUpdateTeam {
    pub name: Option<String>,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub star: Option<bool>,
    pub expected_version: Option<i64>,
}

impl UpdateTeamRequest {
    pub fn validate(self) -> Result<ValidatedUpdateTeam, AppError> {
        let name = self.name.map(|value| required_text("name", value, 255)).transpose()?;
        let school = optional_text("school", self.school, 255)?;
        let seat_no = optional_text("seatNo", self.seat_no, 64)?;
        let group_name = optional_text("groupName", self.group_name, 128)?;
        if name.is_none()
            && school.is_none()
            && seat_no.is_none()
            && group_name.is_none()
            && self.star.is_none()
        {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        if self.expected_version.is_some_and(|version| version < 0) {
            return Err(AppError::validation("expectedVersion", "must not be negative"));
        }
        Ok(ValidatedUpdateTeam {
            name,
            school,
            seat_no,
            group_name,
            star: self.star,
            expected_version: self.expected_version,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamAccountResponse {
    pub user_id: i64,
    pub username: String,
    pub enabled: bool,
    pub password_reset_required: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamResponse {
    pub id: i64,
    pub name: String,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub star: bool,
    pub version: i64,
    pub account: Option<TeamAccountResponse>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct TeamRow {
    pub id: i64,
    pub name: String,
    pub school: Option<String>,
    pub seat_no: Option<String>,
    pub group_name: Option<String>,
    pub star: bool,
    pub version: i64,
    pub account_user_id: Option<i64>,
    pub account_username: Option<String>,
    pub account_enabled: Option<bool>,
    pub account_password_reset_required: Option<bool>,
    pub deleted_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TeamRow {
    pub fn response(self) -> Result<TeamResponse, AppError> {
        let account = match (
            self.account_user_id,
            self.account_username,
            self.account_enabled,
            self.account_password_reset_required,
        ) {
            (Some(user_id), Some(username), Some(enabled), Some(password_reset_required)) => {
                Some(TeamAccountResponse { user_id, username, enabled, password_reset_required })
            }
            (None, None, None, None) => None,
            _ => {
                return Err(AppError::internal_message(
                    "load team account",
                    "incomplete account join",
                ));
            }
        };
        Ok(TeamResponse {
            id: self.id,
            name: self.name,
            school: self.school,
            seat_no: self.seat_no,
            group_name: self.group_name,
            star: self.star,
            version: self.version,
            account,
            deleted_at: self.deleted_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberRequest {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role_name: Option<String>,
}

pub struct ValidatedTeamMember {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role_name: Option<String>,
}

impl TeamMemberRequest {
    pub fn validate(self) -> Result<ValidatedTeamMember, AppError> {
        Ok(ValidatedTeamMember {
            name: required_text("name", self.name, 128)?,
            email: optional_text("email", self.email, 255)?,
            phone: optional_text("phone", self.phone, 64)?,
            role_name: optional_text("roleName", self.role_name, 64)?,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberPatchRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role_name: Option<String>,
}

impl TeamMemberPatchRequest {
    pub fn validate(self) -> Result<ValidatedTeamMemberPatch, AppError> {
        let name = self.name.map(|value| required_text("name", value, 128)).transpose()?;
        let email = optional_text("email", self.email, 255)?;
        let phone = optional_text("phone", self.phone, 64)?;
        let role_name = optional_text("roleName", self.role_name, 64)?;
        if name.is_none() && email.is_none() && phone.is_none() && role_name.is_none() {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        Ok(ValidatedTeamMemberPatch { name, email, phone, role_name })
    }
}

pub struct ValidatedTeamMemberPatch {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberResponse {
    pub id: i64,
    pub team_id: i64,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestTeamAssignmentRequest {
    pub team_id: i64,
    pub participation_type: ParticipationType,
    pub group_name: Option<String>,
}

pub struct ValidatedContestTeamAssignment {
    pub team_id: i64,
    pub participation_type: ParticipationType,
    pub group_name: Option<String>,
}

impl ContestTeamAssignmentRequest {
    pub fn validate(self) -> Result<ValidatedContestTeamAssignment, AppError> {
        if self.team_id <= 0 {
            return Err(AppError::validation("teamId", "must be positive"));
        }
        Ok(ValidatedContestTeamAssignment {
            team_id: self.team_id,
            participation_type: self.participation_type,
            group_name: optional_text("groupName", self.group_name, 128)?,
        })
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ContestTeamResponse {
    pub id: i64,
    pub contest_id: i64,
    pub team_id: i64,
    pub team_name: String,
    pub participation_type: String,
    pub group_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportRequest {
    #[schema(min_items = 1, max_items = 100)]
    pub teams: Vec<CreateTeamRequest>,
    pub contest_id: Option<i64>,
    pub participation_type: Option<ParticipationType>,
    /// Require every generated account in this batch to change its password at
    /// first login. Defaults to `true`; set to `false` only when the operator
    /// distributes the initial passwords out of band and accepts their reuse.
    #[serde(default = "default_require_password_reset")]
    pub require_password_reset: bool,
    pub idempotency_key: String,
}

pub struct ValidatedBatchImport {
    pub teams: Vec<ValidatedCreateTeam>,
    pub contest_id: Option<i64>,
    pub participation_type: ParticipationType,
    pub require_password_reset: bool,
    pub idempotency_key: String,
    pub request_hash: String,
}

impl BatchImportRequest {
    pub fn validate(self) -> Result<ValidatedBatchImport, AppError> {
        if self.teams.is_empty() || self.teams.len() > 100 {
            return Err(AppError::validation("teams", "must contain between 1 and 100 rows"));
        }
        let serialized = serde_json::to_vec(&self)
            .map_err(|error| AppError::internal("serialize team import request", error))?;
        let request_hash = hex::encode(Sha256::digest(serialized));
        let idempotency_key = required_text("idempotencyKey", self.idempotency_key, 128)?;
        if self.contest_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("contestId", "must be positive"));
        }
        let mut teams = self
            .teams
            .into_iter()
            .map(CreateTeamRequest::validate)
            .collect::<Result<Vec<_>, _>>()?;
        for team in &mut teams {
            team.require_password_reset = self.require_password_reset;
        }
        Ok(ValidatedBatchImport {
            teams,
            contest_id: self.contest_id,
            participation_type: self.participation_type.unwrap_or(ParticipationType::Official),
            require_password_reset: self.require_password_reset,
            idempotency_key,
            request_hash,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportRowResponse {
    pub index: usize,
    pub team_id: i64,
    pub user_id: Option<i64>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportResponse {
    pub batch_id: String,
    pub total_requested: usize,
    pub created: Vec<BatchImportRowResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetTeamPasswordRequest {
    #[schema(min_length = 8, max_length = 128, write_only)]
    pub new_password: String,
    /// Require the team to change the password at next login. Defaults to
    /// `true`; set to `false` when the operator delivers the new password out
    /// of band.
    #[serde(default = "default_require_password_reset")]
    pub require_password_reset: bool,
}

pub struct ValidatedResetTeamPassword {
    pub new_password: String,
    pub require_password_reset: bool,
}

impl ResetTeamPasswordRequest {
    pub fn validate(self) -> Result<ValidatedResetTeamPassword, AppError> {
        validate_password("newPassword", &self.new_password)?;
        Ok(ValidatedResetTeamPassword {
            new_password: self.new_password,
            require_password_reset: self.require_password_reset,
        })
    }
}

fn default_require_password_reset() -> bool {
    true
}

fn required_text(field: &'static str, value: String, max_chars: usize) -> Result<String, AppError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(AppError::validation(field, "must be non-blank and within its length limit"));
    }
    Ok(value)
}

fn optional_text(
    field: &'static str,
    value: Option<String>,
    max_chars: usize,
) -> Result<Option<String>, AppError> {
    value
        .map(|value| {
            let value = value.trim().to_owned();
            if value.is_empty() || value.chars().count() > max_chars {
                Err(AppError::validation(field, "must be non-blank and within its length limit"))
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn validate_username(value: String) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    if !(3..=64).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AppError::validation(
            "username",
            "must contain 3 to 64 ASCII letters, digits, dots, underscores, or hyphens",
        ));
    }
    Ok(value)
}

fn validate_password(field: &'static str, value: &str) -> Result<(), AppError> {
    if (8..=128).contains(&value.chars().count()) {
        Ok(())
    } else {
        Err(AppError::validation(field, "must contain between 8 and 128 characters"))
    }
}

#[cfg(test)]
mod tests {
    use crate::features::teams::model::{
        BatchImportRequest, CreateTeamRequest, ParticipationType, UpdateTeamRequest,
    };

    #[test]
    fn account_credentials_must_be_complete() {
        let request = CreateTeamRequest {
            name: "Team".to_owned(),
            school: None,
            seat_no: None,
            group_name: None,
            star: false,
            username: Some("team-1".to_owned()),
            initial_password: None,
            require_password_reset: true,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn empty_team_update_is_rejected() {
        let request = UpdateTeamRequest {
            name: None,
            school: None,
            seat_no: None,
            group_name: None,
            star: None,
            expected_version: Some(1),
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn batch_import_requires_idempotency_key() {
        let request = BatchImportRequest {
            teams: vec![CreateTeamRequest {
                name: "Team".to_owned(),
                school: None,
                seat_no: None,
                group_name: None,
                star: false,
                username: None,
                initial_password: None,
                require_password_reset: true,
            }],
            contest_id: None,
            participation_type: Some(ParticipationType::Official),
            require_password_reset: true,
            idempotency_key: " ".to_owned(),
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn batch_require_password_reset_defaults_to_true_and_overrides_rows() {
        let request = BatchImportRequest {
            teams: vec![CreateTeamRequest {
                name: "Team".to_owned(),
                school: None,
                seat_no: None,
                group_name: None,
                star: false,
                username: Some("team-1".to_owned()),
                initial_password: Some("initial-password".to_owned()),
                require_password_reset: true,
            }],
            contest_id: None,
            participation_type: None,
            require_password_reset: false,
            idempotency_key: "batch-1".to_owned(),
        };
        let validated = request.validate().expect("valid batch import");
        assert!(!validated.require_password_reset);
        assert!(!validated.teams[0].require_password_reset);
    }

    #[test]
    fn reset_team_password_request_defaults_to_required() {
        let request = super::ResetTeamPasswordRequest {
            new_password: "brand-new-password".to_owned(),
            require_password_reset: false,
        };
        let validated = request.validate().expect("valid reset request");
        assert!(!validated.require_password_reset);
        assert_eq!(validated.new_password, "brand-new-password");
    }
}
