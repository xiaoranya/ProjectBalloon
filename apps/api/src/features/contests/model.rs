use std::str::FromStr;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContestStatus {
    Draft,
    FrozenConfig,
    Running,
    Paused,
    Ended,
    Archived,
}

impl ContestStatus {
    #[must_use]
    pub const fn can_reschedule(self) -> bool {
        matches!(self, Self::Draft | Self::FrozenConfig)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::FrozenConfig => "FROZEN_CONFIG",
            Self::Running => "RUNNING",
            Self::Paused => "PAUSED",
            Self::Ended => "ENDED",
            Self::Archived => "ARCHIVED",
        }
    }

    #[must_use]
    pub const fn domain(self) -> project_balloon_domain::ContestState {
        match self {
            Self::Draft => project_balloon_domain::ContestState::Draft,
            Self::FrozenConfig => project_balloon_domain::ContestState::FrozenConfig,
            Self::Running => project_balloon_domain::ContestState::Running,
            Self::Paused => project_balloon_domain::ContestState::Paused,
            Self::Ended => project_balloon_domain::ContestState::Ended,
            Self::Archived => project_balloon_domain::ContestState::Archived,
        }
    }
}

impl FromStr for ContestStatus {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "DRAFT" => Ok(Self::Draft),
            "FROZEN_CONFIG" => Ok(Self::FrozenConfig),
            "RUNNING" => Ok(Self::Running),
            "PAUSED" => Ok(Self::Paused),
            "ENDED" => Ok(Self::Ended),
            "ARCHIVED" => Ok(Self::Archived),
            invalid => Err(AppError::internal("invalid contests.status", invalid)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContestVisibility {
    Private,
    Public,
}

impl ContestVisibility {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "PRIVATE",
            Self::Public => "PUBLIC",
        }
    }
}

impl FromStr for ContestVisibility {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "PRIVATE" => Ok(Self::Private),
            "PUBLIC" => Ok(Self::Public),
            invalid => Err(AppError::internal("invalid contests.visibility", invalid)),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestListQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub sort: Option<String>,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default)]
    pub manageable_only: bool,
}

const fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

pub struct ValidatedContestListQuery {
    pub page: u32,
    pub size: u32,
    pub offset: i64,
    pub include_deleted: bool,
    pub manageable_only: bool,
    pub order_by: &'static str,
}

impl ContestListQuery {
    pub fn validate(self) -> Result<ValidatedContestListQuery, AppError> {
        if !(1..=MAX_PAGE_SIZE).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 500"));
        }
        let order_by = match self.sort.as_deref().unwrap_or("updatedAt,desc") {
            "name,asc" => "name ASC, id ASC",
            "name,desc" => "name DESC, id DESC",
            "createdAt,asc" => "created_at ASC, id ASC",
            "createdAt,desc" => "created_at DESC, id DESC",
            "updatedAt,asc" => "updated_at ASC, id ASC",
            "updatedAt,desc" => "updated_at DESC, id DESC",
            "status,asc" => "status ASC, id ASC",
            "status,desc" => "status DESC, id DESC",
            "startAt,asc" => "start_at ASC NULLS LAST, id ASC",
            "startAt,desc" => "start_at DESC NULLS LAST, id DESC",
            _ => {
                return Err(AppError::validation(
                    "sort",
                    "must use an allowed contest field and direction",
                ));
            }
        };
        Ok(ValidatedContestListQuery {
            page: self.page,
            size: self.size,
            offset: checked_offset(self.page, self.size)?,
            include_deleted: self.include_deleted,
            manageable_only: self.manageable_only,
            order_by,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateContestRequest {
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,
    pub visibility: ContestVisibility,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub freeze_at: Option<OffsetDateTime>,
}

pub struct ValidatedCreateContest {
    pub name: String,
    pub visibility: ContestVisibility,
    pub schedule: Option<ContestSchedule>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestCloneRequest {
    #[schema(min_length = 1, max_length = 120)]
    pub name: String,
    pub visibility: ContestVisibility,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub freeze_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub copy_teams: bool,
}

pub struct ValidatedContestClone {
    pub name: String,
    pub visibility: ContestVisibility,
    pub schedule: Option<ContestSchedule>,
    pub copy_teams: bool,
}

impl ContestCloneRequest {
    pub fn validate(self) -> Result<ValidatedContestClone, AppError> {
        let name = self.name.trim().to_owned();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(AppError::validation("name", "must contain between 1 and 120 characters"));
        }
        let schedule = validate_complete_schedule(self.start_at, self.freeze_at, self.end_at)?;
        Ok(ValidatedContestClone {
            name,
            visibility: self.visibility,
            schedule,
            copy_teams: self.copy_teams,
        })
    }
}

impl CreateContestRequest {
    pub fn validate(self) -> Result<ValidatedCreateContest, AppError> {
        let name = validate_name(self.name)?;
        let schedule = validate_complete_schedule(self.start_at, self.freeze_at, self.end_at)?;
        Ok(ValidatedCreateContest { name, visibility: self.visibility, schedule })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContestRequest {
    #[schema(min_length = 1, max_length = 255)]
    pub name: Option<String>,
    pub visibility: Option<ContestVisibility>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub freeze_at: Option<OffsetDateTime>,
}

pub struct ValidatedUpdateContest {
    pub name: Option<String>,
    pub visibility: Option<ContestVisibility>,
    pub start_at: Option<OffsetDateTime>,
    pub end_at: Option<OffsetDateTime>,
    pub freeze_at: Option<OffsetDateTime>,
}

impl ValidatedUpdateContest {
    #[must_use]
    pub const fn changes_schedule(&self) -> bool {
        self.start_at.is_some() || self.freeze_at.is_some() || self.end_at.is_some()
    }
}

impl UpdateContestRequest {
    pub fn validate(self) -> Result<ValidatedUpdateContest, AppError> {
        let name = self.name.map(validate_name).transpose()?;
        if name.is_none()
            && self.visibility.is_none()
            && self.start_at.is_none()
            && self.end_at.is_none()
            && self.freeze_at.is_none()
        {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        Ok(ValidatedUpdateContest {
            name,
            visibility: self.visibility,
            start_at: self.start_at,
            end_at: self.end_at,
            freeze_at: self.freeze_at,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContestSchedule {
    pub start_at: OffsetDateTime,
    pub freeze_at: OffsetDateTime,
    pub end_at: OffsetDateTime,
}

impl ContestSchedule {
    pub fn validate(self) -> Result<Self, AppError> {
        if self.start_at > self.freeze_at {
            return Err(AppError::validation("startAt", "must not be after freezeAt"));
        }
        if self.freeze_at > self.end_at {
            return Err(AppError::validation("freezeAt", "must not be after endAt"));
        }
        Ok(self)
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestResponse {
    pub id: i64,
    pub name: String,
    pub status: ContestStatus,
    pub visibility: ContestVisibility,
    #[serde(with = "time::serde::rfc3339::option")]
    pub start_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub freeze_at: Option<OffsetDateTime>,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LifecycleTransitionRequest {
    pub to: ContestStatus,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleTransitionResponse {
    pub contest_id: i64,
    pub from: ContestStatus,
    pub to: ContestStatus,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub transitioned_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestCloneResponse {
    pub source_contest_id: i64,
    pub contest: ContestResponse,
    pub problems_copied: i64,
    pub teams_copied: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestExtensionRequest {
    #[serde(with = "time::serde::rfc3339")]
    pub expected_end_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub new_end_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestExtensionResponse {
    pub contest_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub previous_end_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end_at: OffsetDateTime,
    pub version: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct ContestRow {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub visibility: String,
    pub start_at: Option<OffsetDateTime>,
    pub end_at: Option<OffsetDateTime>,
    pub freeze_at: Option<OffsetDateTime>,
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

impl ContestRow {
    pub fn response(self) -> Result<ContestResponse, AppError> {
        Ok(ContestResponse {
            id: self.id,
            name: self.name,
            status: self.status.parse()?,
            visibility: self.visibility.parse()?,
            start_at: self.start_at,
            end_at: self.end_at,
            freeze_at: self.freeze_at,
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        })
    }
}

fn validate_name(value: String) -> Result<String, AppError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > 255 {
        return Err(AppError::validation("name", "must contain between 1 and 255 characters"));
    }
    Ok(value)
}

fn validate_complete_schedule(
    start_at: Option<OffsetDateTime>,
    freeze_at: Option<OffsetDateTime>,
    end_at: Option<OffsetDateTime>,
) -> Result<Option<ContestSchedule>, AppError> {
    match (start_at, freeze_at, end_at) {
        (None, None, None) => Ok(None),
        (Some(start_at), Some(freeze_at), Some(end_at)) => {
            Ok(Some(ContestSchedule { start_at, freeze_at, end_at }.validate()?))
        }
        _ => Err(AppError::validation(
            "schedule",
            "startAt, freezeAt, and endAt must be supplied together",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContestListQuery, ContestSchedule, ContestStatus, ContestVisibility, CreateContestRequest,
    };
    use time::{Duration, OffsetDateTime};

    #[test]
    fn complete_schedule_is_required_on_create() {
        let request = CreateContestRequest {
            name: "Cup".to_owned(),
            visibility: ContestVisibility::Private,
            start_at: Some(OffsetDateTime::UNIX_EPOCH),
            freeze_at: None,
            end_at: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn schedule_order_is_validated() {
        let schedule = ContestSchedule {
            start_at: OffsetDateTime::UNIX_EPOCH + Duration::HOUR,
            freeze_at: OffsetDateTime::UNIX_EPOCH,
            end_at: OffsetDateTime::UNIX_EPOCH + Duration::HOUR * 2,
        };
        assert!(schedule.validate().is_err());
    }

    #[test]
    fn sort_is_allow_listed() {
        assert!(
            ContestListQuery {
                page: 0,
                size: 50,
                sort: Some("evil,asc".to_owned()),
                include_deleted: false,
                manageable_only: false,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn manageable_only_is_preserved_for_service_filtering() {
        let query = ContestListQuery {
            page: 0,
            size: 50,
            sort: None,
            include_deleted: false,
            manageable_only: true,
        }
        .validate()
        .expect("manageable contest query must validate");
        assert!(query.manageable_only);
    }

    #[test]
    fn only_pre_running_statuses_can_reschedule() {
        assert!(ContestStatus::Draft.can_reschedule());
        assert!(ContestStatus::FrozenConfig.can_reschedule());
        assert!(!ContestStatus::Running.can_reschedule());
    }
}
