use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::error::AppError;

const MAX_CONTENT_BYTES: usize = 20 * 1024;
const MAX_PAGES: usize = 5;
const LINES_PER_PAGE: usize = 50;
const COLUMNS_PER_LINE: usize = 100;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    pub(super) content: String,
}

pub(super) struct ValidatedContent {
    pub(super) content: String,
    pub(super) page_count: i32,
    pub(super) hash: String,
}

impl CreateRequest {
    pub(super) fn validate(self) -> Result<ValidatedContent, AppError> {
        let content = self.content.replace("\r\n", "\n").replace('\r', "\n");
        if content.trim().is_empty() || content.len() > MAX_CONTENT_BYTES {
            return Err(AppError::validation("content", "must contain 1 byte to 20 KiB"));
        }
        if content
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        {
            return Err(AppError::validation("content", "contains unsupported control characters"));
        }
        let page_count = estimate_pages(&content);
        if page_count > MAX_PAGES {
            return Err(AppError::validation("content", "must fit within 5 estimated A4 pages"));
        }
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        Ok(ValidatedContent {
            content,
            page_count: i32::try_from(page_count)
                .map_err(|error| AppError::internal("convert print page count", error))?,
            hash,
        })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RejectRequest {
    pub(super) reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    status: Option<String>,
}

impl ListQuery {
    pub(super) fn validate(self) -> Result<Option<String>, AppError> {
        self.status
            .map(|value| {
                let value = value.trim().to_ascii_uppercase();
                if matches!(
                    value.as_str(),
                    "REQUESTED"
                        | "QUEUED"
                        | "PRINTING"
                        | "COMPLETED"
                        | "FAILED"
                        | "CANCELLED"
                        | "REJECTED"
                ) {
                    Ok(value)
                } else {
                    Err(AppError::validation("status", "contains an unsupported print status"))
                }
            })
            .transpose()
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PrintRequestResponse {
    pub(super) id: i64,
    pub(super) contest_id: i64,
    pub(super) team_id: i64,
    pub(super) team_name: Option<String>,
    pub(super) seat_no: Option<String>,
    pub(super) content_hash: String,
    pub(super) page_count: i32,
    pub(super) status: String,
    pub(super) printer_id: Option<String>,
    pub(super) cups_job_id: Option<String>,
    pub(super) requested_by_user_id: i64,
    pub(super) operator_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub(super) completed_at: Option<OffsetDateTime>,
    pub(super) failed_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(super) updated_at: OffsetDateTime,
    pub(super) version: i32,
}

pub(super) fn estimate_pages(content: &str) -> usize {
    let lines = content
        .split('\n')
        .map(|line| {
            let columns =
                line.chars().map(|character| if character == '\t' { 4 } else { 1 }).sum::<usize>();
            columns.max(1).div_ceil(COLUMNS_PER_LINE)
        })
        .sum::<usize>();
    lines.max(1).div_ceil(LINES_PER_PAGE)
}
