use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::auth::{model::UserType, permissions},
    pagination::checked_offset,
};

pub const DEFAULT_PAGE_SIZE: u32 = 100;
pub const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PageQuery {
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub sort: Option<String>,
}

const fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

impl PageQuery {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(1..=MAX_PAGE_SIZE).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 100"));
        }
        if let Some(sort) = &self.sort
            && sort != "username,asc"
        {
            return Err(AppError::validation("sort", "only username,asc is supported"));
        }
        Ok(())
    }

    pub fn offset(&self) -> Result<i64, AppError> {
        checked_offset(self.page, self.size)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateStaffAccountRequest {
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub is_super_admin: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub initial_password: String,
    /// Require the staff account to change its password at first login.
    /// Defaults to `true`.
    #[serde(default = "default_require_password_reset")]
    pub require_password_reset: bool,
}

pub struct ValidatedCreate {
    pub username: String,
    pub display_name: String,
    pub user_type: UserType,
    pub permissions: Vec<String>,
    pub initial_password: String,
    pub require_password_reset: bool,
}

impl CreateStaffAccountRequest {
    pub fn validate(self) -> Result<ValidatedCreate, AppError> {
        let username = self.username.trim().to_ascii_lowercase();
        if !(3..=64).contains(&username.len())
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(AppError::validation(
                "username",
                "must contain 3 to 64 ASCII letters, digits, dots, underscores, or hyphens",
            ));
        }
        let display_name = validate_display_name(self.display_name)?;
        let permissions = validate_permissions(self.permissions)?;
        if self.is_super_admin && !permissions.is_empty() {
            return Err(AppError::validation(
                "permissions",
                "super administrators must not have explicit permissions",
            ));
        }
        validate_password("initialPassword", &self.initial_password)?;
        Ok(ValidatedCreate {
            username,
            display_name,
            user_type: if self.is_super_admin { UserType::SuperAdmin } else { UserType::Staff },
            permissions,
            initial_password: self.initial_password,
            require_password_reset: self.require_password_reset,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStaffAccountRequest {
    pub display_name: Option<String>,
    pub is_super_admin: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

pub struct ValidatedUpdate {
    pub display_name: Option<String>,
    pub is_super_admin: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

impl UpdateStaffAccountRequest {
    pub fn validate(self) -> Result<ValidatedUpdate, AppError> {
        let display_name = self.display_name.map(validate_display_name).transpose()?;
        let permissions = self.permissions.map(validate_permissions).transpose()?;
        if display_name.is_none()
            && self.is_super_admin.is_none()
            && permissions.is_none()
            && self.enabled.is_none()
        {
            return Err(AppError::validation("request", "must include at least one change"));
        }
        Ok(ValidatedUpdate {
            display_name,
            is_super_admin: self.is_super_admin,
            permissions,
            enabled: self.enabled,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetStaffPasswordRequest {
    pub new_password: String,
    /// Require the staff account to change its password at next login.
    /// Defaults to `true`.
    #[serde(default = "default_require_password_reset")]
    pub require_password_reset: bool,
}

pub struct ValidatedResetStaffPassword {
    pub new_password: String,
    pub require_password_reset: bool,
}

impl ResetStaffPasswordRequest {
    pub fn validate(self) -> Result<ValidatedResetStaffPassword, AppError> {
        validate_password("newPassword", &self.new_password)?;
        Ok(ValidatedResetStaffPassword {
            new_password: self.new_password,
            require_password_reset: self.require_password_reset,
        })
    }
}

fn default_require_password_reset() -> bool {
    true
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StaffAccountResponse {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub user_type: UserType,
    pub permissions: Vec<String>,
    pub enabled: bool,
    pub password_reset_required: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_login_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct StaffAccountRow {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub user_type: String,
    pub permissions: Vec<String>,
    pub enabled: bool,
    pub password_reset_required: bool,
    pub last_login_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl StaffAccountRow {
    pub fn response(self) -> Result<StaffAccountResponse, AppError> {
        Ok(StaffAccountResponse {
            id: self.id,
            username: self.username,
            display_name: self.display_name,
            user_type: self.user_type.parse()?,
            permissions: self.permissions,
            enabled: self.enabled,
            password_reset_required: self.password_reset_required,
            last_login_at: self.last_login_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn validate_display_name(value: String) -> Result<String, AppError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > 128 {
        return Err(AppError::validation(
            "displayName",
            "must contain between 1 and 128 characters",
        ));
    }
    Ok(value)
}

fn validate_permissions(values: Vec<String>) -> Result<Vec<String>, AppError> {
    let values = values.into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
    if values.iter().any(|code| !permissions::ASSIGNABLE.contains(&code.as_str())) {
        return Err(AppError::validation("permissions", "contains an unknown permission"));
    }
    Ok(values)
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
    use crate::features::staff_accounts::model::{
        CreateStaffAccountRequest, PageQuery, ResetStaffPasswordRequest, UpdateStaffAccountRequest,
    };
    #[test]
    fn create_normalizes_username_and_display_name() {
        let validated = CreateStaffAccountRequest {
            username: " Staff.One ".to_owned(),
            display_name: " Operator ".to_owned(),
            is_super_admin: false,
            permissions: vec!["PRINTING_MANAGE".to_owned()],
            initial_password: "temporary-password".to_owned(),
            require_password_reset: true,
        }
        .validate()
        .expect("valid account");
        assert_eq!(validated.username, "staff.one");
        assert_eq!(validated.display_name, "Operator");
    }

    #[test]
    fn create_require_password_reset_is_forwarded() {
        let validated = CreateStaffAccountRequest {
            username: "staff.two".to_owned(),
            display_name: "Operator Two".to_owned(),
            is_super_admin: false,
            permissions: vec![],
            initial_password: "temporary-password".to_owned(),
            require_password_reset: false,
        }
        .validate()
        .expect("valid account");
        assert!(!validated.require_password_reset);
    }

    #[test]
    fn unknown_permissions_are_rejected() {
        let request = CreateStaffAccountRequest {
            username: "team-user".to_owned(),
            display_name: "Team".to_owned(),
            is_super_admin: false,
            permissions: vec!["UNKNOWN".to_owned()],
            initial_password: "temporary-password".to_owned(),
            require_password_reset: true,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn empty_update_is_rejected() {
        let request = UpdateStaffAccountRequest {
            display_name: None,
            is_super_admin: None,
            permissions: None,
            enabled: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn page_size_and_password_are_bounded() {
        assert!(PageQuery { page: 0, size: 101, sort: None }.validate().is_err());
        assert!(
            ResetStaffPasswordRequest {
                new_password: "short".to_owned(),
                require_password_reset: true,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn reset_require_password_reset_is_forwarded() {
        let validated = ResetStaffPasswordRequest {
            new_password: "brand-new-password".to_owned(),
            require_password_reset: false,
        }
        .validate()
        .expect("valid reset request");
        assert!(!validated.require_password_reset);
        assert_eq!(validated.new_password, "brand-new-password");
    }
}
