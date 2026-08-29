use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;
use crate::features::competition::model::CompetitionSessionResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserType {
    SuperAdmin,
    Staff,
    Team,
    Individual,
}

impl UserType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SuperAdmin => "SUPER_ADMIN",
            Self::Staff => "STAFF",
            Self::Team => "TEAM",
            Self::Individual => "INDIVIDUAL",
        }
    }

    #[must_use]
    pub const fn is_staff(self) -> bool {
        !matches!(self, Self::Team | Self::Individual)
    }
}

impl FromStr for UserType {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "SUPER_ADMIN" => Ok(Self::SuperAdmin),
            "STAFF" => Ok(Self::Staff),
            "TEAM" => Ok(Self::Team),
            "INDIVIDUAL" => Ok(Self::Individual),
            invalid => Err(AppError::internal_message("invalid users.user_type", invalid)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub user_type: UserType,
    pub permissions: Vec<String>,
    pub password_reset_required: bool,
}

impl AuthUser {
    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        self.user_type == UserType::SuperAdmin
            || self.permissions.iter().any(|code| code == permission)
    }

    #[must_use]
    pub const fn is_super_admin(&self) -> bool {
        matches!(self.user_type, UserType::SuperAdmin)
    }

    #[must_use]
    pub fn response(&self) -> CurrentUserResponse {
        CurrentUserResponse {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            user_type: self.user_type,
            permissions: self.permissions.clone(),
            password_reset_required: self.password_reset_required,
            competition: None,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUserResponse {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub user_type: UserType,
    pub permissions: Vec<String>,
    pub password_reset_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competition: Option<CompetitionSessionResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    #[schema(min_length = 1, max_length = 64)]
    pub username: String,
    #[schema(min_length = 1, max_length = 128, write_only)]
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterRequest {
    #[schema(min_length = 3, max_length = 64)]
    pub username: String,
    #[schema(min_length = 8, max_length = 128, write_only)]
    pub password: String,
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
}

impl RegisterRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.username.trim().len() < 3
            || self.username.trim().len() > 64
            || !self
                .username
                .trim()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Err(AppError::validation(
                "username",
                "must use 3-64 letters, digits, dots, hyphens, or underscores",
            ));
        }
        let length = self.password.chars().count();
        if !(8..=128).contains(&length) {
            return Err(AppError::validation(
                "password",
                "must contain between 8 and 128 characters",
            ));
        }
        if self.display_name.trim().is_empty() || self.display_name.trim().chars().count() > 128 {
            return Err(AppError::validation(
                "displayName",
                "must be non-empty and at most 128 characters",
            ));
        }
        Ok(())
    }
}

impl LoginRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_non_blank("username", &self.username)?;
        validate_non_blank("password", &self.password)?;
        if self.username.chars().count() > 64 {
            return Err(AppError::validation("username", "must contain at most 64 characters"));
        }
        if self.password.chars().count() > 128 {
            return Err(AppError::validation("password", "must contain at most 128 characters"));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordRequest {
    #[schema(min_length = 1, max_length = 128, write_only)]
    pub current_password: String,
    #[schema(min_length = 8, max_length = 128, write_only)]
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
}

impl ProfileRequest {
    pub fn validate(&self) -> Result<String, AppError> {
        let display_name = self.display_name.trim();
        if display_name.is_empty() || display_name.chars().count() > 128 {
            return Err(AppError::validation(
                "displayName",
                "must be non-empty and at most 128 characters",
            ));
        }
        Ok(display_name.to_owned())
    }
}

impl ChangePasswordRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_non_blank("currentPassword", &self.current_password)?;
        let length = self.new_password.chars().count();
        if !(8..=128).contains(&length) {
            return Err(AppError::validation(
                "newPassword",
                "must contain between 8 and 128 characters",
            ));
        }
        Ok(())
    }
}

fn validate_non_blank(field: &'static str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::validation(field, "must not be blank"));
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct UserRow {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub user_type: String,
    pub enabled: bool,
    pub password_reset_required: bool,
    pub permissions: Vec<String>,
}

impl UserRow {
    pub fn auth_user(&self) -> Result<AuthUser, AppError> {
        Ok(AuthUser {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            user_type: self.user_type.parse()?,
            permissions: self.permissions.clone(),
            password_reset_required: self.password_reset_required,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangePasswordRequest, LoginRequest, ProfileRequest, UserType};

    #[test]
    fn user_type_wire_names_are_stable() {
        assert_eq!(UserType::SuperAdmin.as_str(), "SUPER_ADMIN");
        assert_eq!("STAFF".parse::<UserType>().expect("valid type"), UserType::Staff);
    }

    #[test]
    fn login_rejects_blank_values() {
        let request = LoginRequest { username: " ".to_owned(), password: "secret".to_owned() };
        assert!(request.validate().is_err());
    }

    #[test]
    fn password_length_is_bounded() {
        let request = ChangePasswordRequest {
            current_password: "current".to_owned(),
            new_password: "short".to_owned(),
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn profile_name_is_trimmed_and_bounded() {
        let request = ProfileRequest { display_name: "  Daily User  ".to_owned() };
        assert_eq!(request.validate().expect("valid profile"), "Daily User");
        assert!(ProfileRequest { display_name: " ".to_owned() }.validate().is_err());
    }
}
