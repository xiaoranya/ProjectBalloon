use std::net::SocketAddr;

use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::Response,
};

use crate::{error::AppError, features::auth::ContestManagerContext, state::AppState};

use crate::features::submissions::handlers::{export_response, required_storage};
use crate::features::submissions::model::{RejudgeRequest, RejudgeResponse};
use crate::features::submissions::{CreateExportTaskRequest, ExportTaskResponse};

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/submissions/{submission_id}/rejudge",
    operation_id = "rejudgeSubmission",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("submission_id" = i64, Path)),
    request_body = RejudgeRequest,
    responses(
        (status = 202, description = "Submission rejudge queued", body = RejudgeResponse),
        (status = 400, description = "Invalid optimistic judgement ID", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Submission was not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Submission state or optimistic version conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn rejudge(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest_id, submission_id)): Path<(i64, i64)>,
    payload: Result<Json<RejudgeRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RejudgeResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid rejudge request"))?;
    let response = state
        .submissions()
        .rejudge(contest_id, submission_id, request, context.user(), peer.ip())
        .await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/exports/submissions.csv",
    operation_id = "exportSubmissionMetadataCsv",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    responses(
        (status = 200, description = "Submission metadata CSV", body = Vec<u8>, content_type = "text/csv"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn export_metadata_csv(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    let csv =
        state.submissions().export_metadata_csv(contest_id, context.user(), peer.ip()).await?;
    export_response(contest_id, "submissions.csv", "text/csv; charset=utf-8", csv)
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/exports/submission-sources.zip",
    operation_id = "exportSubmissionSourcesZip",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    responses(
        (status = 200, description = "Submission source ZIP", body = Vec<u8>, content_type = "application/zip"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside management scope", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn export_sources_zip(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    let storage = required_storage(&state)?;
    let archive = state
        .submissions()
        .export_sources_zip(contest_id, context.user(), peer.ip(), storage)
        .await?;
    export_response(contest_id, "submission-sources.zip", "application/zip", archive)
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/exports/tasks",
    operation_id = "createSubmissionExportTask",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    request_body = CreateExportTaskRequest,
    responses(
        (status = 202, body = ExportTaskResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create_export_task(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateExportTaskRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ExportTaskResponse>), AppError> {
    required_storage(&state)?;
    let request = payload.map_err(|_| AppError::validation("request", "must be valid JSON"))?.0;
    let response =
        state.submissions().create_export_task(contest_id, context.user(), request.kind).await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/exports/tasks/{task_id}",
    operation_id = "getSubmissionExportTask",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("task_id" = i64, Path)),
    responses(
        (status = 200, body = ExportTaskResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn get_export_task(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, task_id)): Path<(i64, i64)>,
) -> Result<Json<ExportTaskResponse>, AppError> {
    Ok(Json(state.submissions().get_export_task(contest_id, task_id, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/exports/tasks/{task_id}/download",
    operation_id = "downloadSubmissionExportTask",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("task_id" = i64, Path)),
    responses(
        (status = 200, description = "Generated export artifact", body = Vec<u8>, content_type = "application/octet-stream"),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn download_export_task(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, task_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    let task = state.submissions().get_export_task(contest_id, task_id, context.user()).await?;
    if task.status != "SUCCEEDED" {
        return Err(AppError::conflict(
            "EXPORT_TASK_NOT_READY",
            "Export task is not ready for download",
        ));
    }
    if task.expires_at.is_some_and(|expires_at| expires_at <= time::OffsetDateTime::now_utc()) {
        return Err(AppError::conflict("EXPORT_TASK_EXPIRED", "Export task has expired"));
    }
    let bucket = task.output_bucket.as_deref().ok_or_else(|| {
        AppError::conflict("EXPORT_TASK_NOT_READY", "Export task has no output object")
    })?;
    let key = task.output_object_key.as_deref().ok_or_else(|| {
        AppError::conflict("EXPORT_TASK_NOT_READY", "Export task has no output object")
    })?;
    let stream = required_storage(&state)?
        .backend()
        .get_stream_limited(bucket, key, 2 * 1024 * 1024 * 1024)
        .await
        .map_err(|error| AppError::internal("download export task output", error))?;
    let (suffix, content_type) = if task.kind == "METADATA_CSV" {
        ("submissions.csv", "text/csv; charset=utf-8")
    } else {
        ("submission-sources.zip", "application/zip")
    };
    export_response(contest_id, suffix, content_type, Body::from_stream(stream))
}
