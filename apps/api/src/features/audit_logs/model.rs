use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{error::AppError, pagination::checked_offset};

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogQuery {
    pub actor_user_id: Option<i64>,
    pub action: Option<String>,
    pub result: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub from: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub to: Option<OffsetDateTime>,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub size: u32,
    pub sort: Option<String>,
}

const fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

pub struct ValidatedAuditLogQuery {
    pub actor_user_id: Option<i64>,
    pub action_pattern: Option<String>,
    pub result: Option<String>,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
    pub page: u32,
    pub size: u32,
    pub offset: i64,
}

impl AuditLogQuery {
    pub fn validate(self) -> Result<ValidatedAuditLogQuery, AppError> {
        if self.actor_user_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("actorUserId", "must be a positive ID"));
        }
        if !(1..=MAX_PAGE_SIZE).contains(&self.size) {
            return Err(AppError::validation("size", "must contain a value between 1 and 100"));
        }
        if let Some(sort) = &self.sort
            && sort != "createdAt,desc"
        {
            return Err(AppError::validation("sort", "only createdAt,desc is supported"));
        }
        if self.from.zip(self.to).is_some_and(|(from, to)| from > to) {
            return Err(AppError::validation("from", "must not be after to"));
        }
        let action = normalize_filter("action", self.action, 128)?;
        let result = normalize_filter("result", self.result, 32)?;
        let action_pattern = action.map(|value| format!("%{}%", escape_like(&value)));
        let offset = checked_offset(self.page, self.size)?;
        Ok(ValidatedAuditLogQuery {
            actor_user_id: self.actor_user_id,
            action_pattern,
            result,
            from: self.from,
            to: self.to,
            page: self.page,
            size: self.size,
            offset,
        })
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogResponse {
    pub id: i64,
    pub actor_user_id: Option<i64>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub request_ip: Option<String>,
    pub result: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

fn normalize_filter(
    field: &'static str,
    value: Option<String>,
    max_length: usize,
) -> Result<Option<String>, AppError> {
    let value = value.map(|value| value.trim().to_lowercase());
    match value {
        Some(value) if value.chars().count() > max_length => {
            Err(AppError::validation(field, "filter is too long"))
        }
        Some(value) if value.is_empty() => Ok(None),
        value => Ok(value),
    }
}

fn escape_like(value: &str) -> String {
    value.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::{AuditLogQuery, escape_like};

    #[test]
    fn like_metacharacters_are_escaped() {
        assert_eq!(escape_like(r"login_%\done"), r"login\_\%\\done");
    }

    #[test]
    fn invalid_range_is_rejected() {
        let query = AuditLogQuery {
            actor_user_id: None,
            action: None,
            result: None,
            from: Some(time::OffsetDateTime::UNIX_EPOCH + time::Duration::SECOND),
            to: Some(time::OffsetDateTime::UNIX_EPOCH),
            page: 0,
            size: 25,
            sort: None,
        };
        assert!(query.validate().is_err());
    }
}
