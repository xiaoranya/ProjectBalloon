use std::borrow::Cow;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default)]
pub struct InternalContext {
    pub contest_id: Option<i64>,
    pub submission_id: Option<i64>,
    pub judgement_id: Option<Uuid>,
    pub user_id: Option<i64>,
}

impl InternalContext {
    #[must_use]
    pub const fn new() -> Self {
        Self { contest_id: None, submission_id: None, judgement_id: None, user_id: None }
    }

    #[must_use]
    pub const fn contest_id(mut self, id: i64) -> Self {
        self.contest_id = Some(id);
        self
    }

    #[must_use]
    pub const fn submission_id(mut self, id: i64) -> Self {
        self.submission_id = Some(id);
        self
    }

    #[must_use]
    pub fn judgement_id(mut self, id: Uuid) -> Self {
        self.judgement_id = Some(id);
        self
    }

    #[must_use]
    pub const fn user_id(mut self, id: i64) -> Self {
        self.user_id = Some(id);
        self
    }
}

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
    internal_detail: Option<Box<str>>,
    // Boxed so the `Err` variant stays under clippy's result_large_err limit;
    // internal errors allocate anyway, so the extra indirection is free.
    internal_context: Option<Box<InternalContext>>,
}

impl AppError {
    #[must_use]
    pub fn code(&self) -> &str {
        self.body.code.as_ref()
    }

    /// Attaches business identifiers that are only useful for the structured
    /// internal-error log; the HTTP response body is untouched.
    #[must_use]
    pub fn with_contest_id(mut self, id: i64) -> Self {
        self.internal_context().contest_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_submission_id(mut self, id: i64) -> Self {
        self.internal_context().submission_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_judgement_id(mut self, id: Uuid) -> Self {
        self.internal_context().judgement_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_user_id(mut self, id: i64) -> Self {
        self.internal_context().user_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_internal_context(mut self, context: InternalContext) -> Self {
        self.internal_context = Some(Box::new(context));
        self
    }

    fn internal_context(&mut self) -> &mut InternalContext {
        self.internal_context.get_or_insert_with(|| Box::new(InternalContext::new()))
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
            internal_context: None,
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

    pub fn internal(context: &'static str, source: impl std::error::Error + 'static) -> Self {
        if is_archived_read_only_database_error(&source) {
            return Self::conflict(
                ARCHIVED_READ_ONLY_MESSAGE,
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
            internal_detail: Some(format!("{context}: {source}").into_boxed_str()),
            internal_context: None,
        }
    }

    /// Internal failure raised from a non-`Error` payload, such as an
    /// unexpected in-memory value. Database-backed failures must go through
    /// [`AppError::internal`] so archived-contest conflicts stay typed.
    pub fn internal_message(context: &'static str, message: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: ApiErrorBody {
                code: Cow::Borrowed("INTERNAL_ERROR"),
                message: Cow::Borrowed("An internal error occurred while processing the request"),
                field_errors: Vec::new(),
            },
            internal_detail: Some(format!("{context}: {message}").into_boxed_str()),
            internal_context: None,
        }
    }

    fn with_internal_detail(mut self, detail: String) -> Self {
        self.internal_detail = Some(detail.into_boxed_str());
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
            internal_context: None,
        }
    }
}

const ARCHIVED_READ_ONLY_MESSAGE: &str = "CONTEST_ARCHIVED_READ_ONLY";

fn is_archived_read_only_database_error(error: &(impl std::error::Error + 'static)) -> bool {
    let error: &dyn std::any::Any = error;
    error
        .downcast_ref::<sqlx::Error>()
        .and_then(|error| error.as_database_error())
        .is_some_and(|database_error| database_error.message().trim() == ARCHIVED_READ_ONLY_MESSAGE)
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Some(detail) = &self.internal_detail {
            // Option fields are only recorded when present, so the log line
            // carries exactly the identifiers attached upstream.
            match &self.internal_context {
                Some(context) => error!(
                    %detail,
                    contest_id = context.contest_id,
                    submission_id = context.submission_id,
                    judgement_id = context.judgement_id.map(tracing::field::display),
                    user_id = context.user_id.map(tracing::field::display),
                    "request failed"
                ),
                None => error!(%detail, "request failed"),
            }
        }
        (self.status, Json(self.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_database_sqlx_error_stays_internal() {
        let error = AppError::internal("query", sqlx::Error::RowNotFound);

        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.code(), "INTERNAL_ERROR");
        assert_eq!(
            error.internal_detail.as_deref(),
            Some("query: no rows returned by a query that expected to return at least one row")
        );
    }

    #[test]
    fn internal_message_stays_internal() {
        let error = AppError::internal_message("build plan", "snapshot has no SHA-256");

        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.code(), "INTERNAL_ERROR");
        assert_eq!(error.internal_detail.as_deref(), Some("build plan: snapshot has no SHA-256"));
    }
}
