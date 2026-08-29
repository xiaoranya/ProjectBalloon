use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{error::AppError, features::auth::ContestManagerContext, state::AppState};

use crate::features::submissions::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgePreviewResponse,
    BatchRejudgeTaskResponse,
};

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks/preview",
    operation_id = "previewBatchRejudge",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    request_body = BatchRejudgeFilter,
    responses(
        (status = 200, description = "Number of submissions matching the filter", body = BatchRejudgePreviewResponse),
        (status = 400, description = "Invalid rejudge filter", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn preview_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<BatchRejudgeFilter>, JsonRejection>,
) -> Result<Json<BatchRejudgePreviewResponse>, AppError> {
    let Json(filter) =
        payload.map_err(|_| AppError::validation("request", "must be a valid rejudge filter"))?;
    Ok(Json(state.batch_rejudge().preview(contest_id, context.user(), filter).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks",
    operation_id = "createBatchRejudge",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    request_body = BatchRejudgeCreateRequest,
    responses(
        (status = 202, description = "Batch rejudge task created", body = BatchRejudgeTaskResponse),
        (status = 400, description = "Invalid filter, count, confirmation, or idempotency key", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Idempotency or active batch conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn create_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<BatchRejudgeCreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<BatchRejudgeTaskResponse>), AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid batch rejudge request"))?;
    Ok((
        StatusCode::ACCEPTED,
        Json(state.batch_rejudge().create(contest_id, context.user(), request).await?),
    ))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks",
    operation_id = "listBatchRejudges",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    responses(
        (status = 200, description = "Batch rejudge tasks", body = [BatchRejudgeTaskResponse]),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<BatchRejudgeTaskResponse>>, AppError> {
    Ok(Json(state.batch_rejudge().list(contest_id, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}",
    operation_id = "getBatchRejudge",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("task_id" = i64, Path)),
    responses(
        (status = 200, description = "Batch rejudge task and item status", body = BatchRejudgeTaskResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Task is not found or outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn get_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, task_id)): Path<(i64, i64)>,
) -> Result<Json<BatchRejudgeTaskResponse>, AppError> {
    Ok(Json(state.batch_rejudge().get(contest_id, task_id, context.user()).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/pause",
    operation_id = "pauseBatchRejudge",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("task_id" = i64, Path)),
    responses(
        (status = 200, description = "Batch rejudge pause requested", body = BatchRejudgeTaskResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Task is not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Task cannot be paused in its current state", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn pause_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, task_id)): Path<(i64, i64)>,
) -> Result<Json<BatchRejudgeTaskResponse>, AppError> {
    Ok(Json(state.batch_rejudge().pause(contest_id, task_id, context.user()).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/resume",
    operation_id = "resumeBatchRejudge",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("task_id" = i64, Path)),
    responses(
        (status = 200, description = "Batch rejudge resumed", body = BatchRejudgeTaskResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Task is not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Task cannot be resumed in its current state", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn resume_batch_rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, task_id)): Path<(i64, i64)>,
) -> Result<Json<BatchRejudgeTaskResponse>, AppError> {
    Ok(Json(state.batch_rejudge().resume(contest_id, task_id, context.user()).await?))
}
