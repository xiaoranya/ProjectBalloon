use std::borrow::Cow;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiErrorBody {
    code: Cow<'static, str>,
    message: Cow<'static, str>,
    field_errors: Vec<FieldError>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct FieldError {
    field: Cow<'static, str>,
    message: Cow<'static, str>,
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    body: ApiErrorBody,
    internal_detail: Option<String>,
}

impl AppError {
    #[must_use]
    pub fn code(&self) -> &str {
        self.body.code.as_ref()
    }

    pub fn validation(field: &'static str, message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: ApiErrorBody {
                code: Cow::Borrowed("VALIDATION_FAILED"),
                message: Cow::Borrowed("Validation failed"),
                field_errors: vec![FieldError {
                    field: Cow::Borrowed(field),
                    message: Cow::Borrowed(message),
                }],
            },
            internal_detail: None,
        }
    }

    pub fn bad_request(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn unauthorized(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::UNAUTHORIZED, code, message)
    }

    pub fn forbidden(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::CONFLICT, code, message)
    }

    pub fn too_many_requests(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::TOO_MANY_REQUESTS, code, message)
    }

    pub fn service_unavailable(code: &'static str, message: &'static str) -> Self {
        Self::public(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }

    pub fn internal(context: &'static str, source: impl std::fmt::Display) -> Self {
        let source = source.to_string();
        if is_archived_read_only_database_error(&source) {
            return Self::conflict(
                "CONTEST_ARCHIVED_READ_ONLY",
                "Archived contest data is read-only",
            )
            .with_internal_detail(format!("{context}: {source}"));
        }
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ApiErrorBody {
                code: Cow::Borrowed("INTERNAL_ERROR"),
                message: Cow::Borrowed("An internal error occurred while processing the request"),
                field_errors: Vec::new(),
            },
            internal_detail: Some(format!("{context}: {source}")),
        }
    }

    fn with_internal_detail(mut self, detail: String) -> Self {
        self.internal_detail = Some(detail);
        self
    }

    fn public(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: Cow::Borrowed(code),
                message: Cow::Borrowed(message),
                field_errors: Vec::new(),
            },
            internal_detail: None,
        }
    }
}

fn is_archived_read_only_database_error(source: &str) -> bool {
    let source = source.trim();
    source == "CONTEST_ARCHIVED_READ_ONLY"
        || source
            .strip_prefix("database error: ")
            .is_some_and(|message| message.trim() == "CONTEST_ARCHIVED_READ_ONLY")
        || source
            .strip_prefix("error returned from database: ")
            .is_some_and(|message| message.trim() == "CONTEST_ARCHIVED_READ_ONLY")
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Some(detail) = &self.internal_detail {
            error!(%detail, "request failed");
        }
        (self.status, Json(self.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_database_rejection_is_exposed_as_conflict() {
        let error =
            AppError::internal("update child row", "database error: CONTEST_ARCHIVED_READ_ONLY");

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.code(), "CONTEST_ARCHIVED_READ_ONLY");
        assert_eq!(
            error.internal_detail.as_deref(),
            Some("update child row: database error: CONTEST_ARCHIVED_READ_ONLY")
        );
    }

    #[test]
    fn unrelated_error_containing_archived_marker_stays_internal() {
        let error =
            AppError::internal("query", "constraint mentions CONTEST_ARCHIVED_READ_ONLY_SUFFIX");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.code(), "INTERNAL_ERROR");
    }
}
