use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AskRequest {
    pub(super) scope: String,
    pub(super) problem_id: Option<i64>,
    pub(super) question: String,
}

pub(super) struct ValidatedAsk {
    pub(super) scope: &'static str,
    pub(super) problem_id: Option<i64>,
    pub(super) question: String,
}

impl AskRequest {
    pub(super) fn validate(mut self) -> Result<ValidatedAsk, AppError> {
        let scope = match self.scope.trim().to_ascii_uppercase().as_str() {
            "GENERAL" if self.problem_id.is_none() => "GENERAL",
            "PROBLEM" if self.problem_id.is_some_and(|id| id > 0) => "PROBLEM",
            "GENERAL" | "PROBLEM" => {
                return Err(AppError::validation(
                    "problemId",
                    "must be absent for GENERAL and positive for PROBLEM",
                ));
            }
            _ => return Err(AppError::validation("scope", "must be GENERAL or PROBLEM")),
        };
        self.question = self.question.trim().to_owned();
        if self.question.is_empty() || self.question.chars().count() > 4000 {
            return Err(AppError::validation("question", "must contain 1 to 4000 characters"));
        }
        Ok(ValidatedAsk { scope, problem_id: self.problem_id, question: self.question })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplyRequest {
    pub(super) reply: String,
    pub(super) visibility: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConvertRequest {
    pub(super) title: Option<String>,
    pub(super) body: Option<String>,
}

pub(super) struct ValidatedReply {
    pub(super) reply: String,
    pub(super) visibility: &'static str,
}

impl ReplyRequest {
    pub(super) fn validate(mut self) -> Result<ValidatedReply, AppError> {
        self.reply = self.reply.trim().to_owned();
        if self.reply.is_empty() || self.reply.chars().count() > 8000 {
            return Err(AppError::validation("reply", "must contain 1 to 8000 characters"));
        }
        let visibility = match self.visibility.trim().to_ascii_uppercase().as_str() {
            "PRIVATE" => "PRIVATE",
            "PUBLIC" => "PUBLIC",
            _ => return Err(AppError::validation("visibility", "must be PRIVATE or PUBLIC")),
        };
        Ok(ValidatedReply { reply: self.reply, visibility })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAllQuery {
    status: Option<String>,
}

impl ListAllQuery {
    pub(super) fn validate(self) -> Result<Option<String>, AppError> {
        self.status
            .map(|status| match status.trim().to_ascii_lowercase().as_str() {
                "pending" => Ok("PENDING".into()),
                "answered" => Ok("ANSWERED".into()),
                "closed" => Ok("CLOSED".into()),
                _ => Err(AppError::validation("status", "must be pending, answered, or closed")),
            })
            .transpose()
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationResponse {
    pub(super) id: i64,
    pub(super) contest_id: i64,
    pub(super) team_id: i64,
    pub(super) team_name: Option<String>,
    pub(super) scope: String,
    pub(super) problem_id: Option<i64>,
    pub(super) problem_alias: Option<String>,
    pub(super) question: String,
    pub(super) status: String,
    pub(super) reply: Option<String>,
    pub(super) reply_visibility: Option<String>,
    pub(super) asked_by_user_id: i64,
    pub(super) replied_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) replied_at: Option<OffsetDateTime>,
    pub(super) converted_announcement_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) updated_at: OffsetDateTime,
    pub(super) version: i32,
}
