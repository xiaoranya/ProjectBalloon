mod batch_rejudge;
mod detail;
mod query;
mod rejudge;
mod submit;

use axum::{
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use crate::{error::AppError, state::AppState};

pub use batch_rejudge::*;
pub use detail::*;
pub use query::*;
pub use rejudge::*;
pub use submit::*;

fn required_storage(
    state: &AppState,
) -> Result<&crate::object_storage::ObjectStorageHandle, AppError> {
    state.object_storage().ok_or_else(|| {
        AppError::service_unavailable(
            "OBJECT_STORAGE_UNAVAILABLE",
            "Object storage is not configured",
        )
    })
}

fn export_response(
    contest_id: i64,
    suffix: &str,
    content_type: &'static str,
    body: impl IntoResponse,
) -> Result<Response, AppError> {
    let disposition =
        HeaderValue::from_str(&format!("attachment; filename=contest-{contest_id}-{suffix}"))
            .map_err(|error| AppError::internal("build submission export filename", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CONTENT_DISPOSITION, disposition),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        ],
        body,
    )
        .into_response())
}
