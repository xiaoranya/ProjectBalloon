use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext},
    state::AppState,
};

use super::super::model::SubmissionDetail;
use super::required_storage;

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/submissions/{submission_id}",
    operation_id = "getOwnSubmission",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("submission_id" = i64, Path)),
    responses(
        (status = 200, description = "Submission detail including source and judgement history", body = SubmissionDetail),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Submission was not found or does not belong to the team", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn detail_own(
    context: AuthContext,
    State(state): State<AppState>,
    Path((contest_id, submission_id)): Path<(i64, i64)>,
) -> Result<Json<SubmissionDetail>, AppError> {
    context.require_password_ready()?;
    let storage = required_storage(&state)?;
    Ok(Json(
        state.submissions().detail_own(contest_id, submission_id, context.user(), storage).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/submissions/{submission_id}",
    operation_id = "getAdminSubmission",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("submission_id" = i64, Path)),
    responses(
        (status = 200, description = "Administrative submission detail including source and judgement history", body = SubmissionDetail),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Submission was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn detail_admin(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, submission_id)): Path<(i64, i64)>,
) -> Result<Json<SubmissionDetail>, AppError> {
    let storage = required_storage(&state)?;
    Ok(Json(
        state
            .submissions()
            .detail_admin(contest_id, submission_id, context.user(), storage)
            .await?,
    ))
}
