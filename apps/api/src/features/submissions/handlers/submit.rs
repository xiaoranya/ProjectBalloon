use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Multipart, Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
};

use crate::{
    error::AppError, features::auth::AuthContext, pagination::PageResponse, state::AppState,
};

use super::super::model::{
    PracticeProblemStatus, PracticeSubmissionDetail, PracticeSubmissionSummary,
    PracticeSubmitMetadata, SubmissionListQuery, SubmissionUploadRequest, SubmitMetadata,
    SubmitResponse,
};
use super::required_storage;

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/submissions",
    operation_id = "submitSource",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    request_body(content = inline(SubmissionUploadRequest), content_type = "multipart/form-data"),
    responses(
        (status = 202, description = "Submission accepted and queued for judging", body = SubmitResponse),
        (status = 400, description = "Invalid metadata, source, language, or filename", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Completed password reset or CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest or problem was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest is not accepting submissions", body = crate::error::ApiErrorBody),
        (status = 503, description = "Object storage is unavailable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn submit(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<SubmitResponse>), AppError> {
    context.require_password_ready()?;
    let mut metadata = None;
    let mut source = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::validation("request", "must be valid multipart data"))?
    {
        match field.name() {
            Some("metadata") if metadata.is_none() => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| AppError::validation("metadata", "must be valid UTF-8 JSON"))?;
                metadata = Some(serde_json::from_str::<SubmitMetadata>(&text).map_err(|_| {
                    AppError::validation("metadata", "must be a valid submission object")
                })?);
            }
            Some("source") if source.is_none() => {
                let filename = field
                    .file_name()
                    .ok_or_else(|| AppError::validation("source", "must have a filename"))?
                    .to_owned();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::validation("source", "could not be read"))?;
                source = Some((filename, bytes));
            }
            _ => {
                return Err(AppError::validation(
                    "request",
                    "must contain exactly one metadata field and one source field",
                ));
            }
        }
    }
    let metadata = metadata.ok_or_else(|| AppError::validation("metadata", "is required"))?;
    let (filename, source) = source.ok_or_else(|| AppError::validation("source", "is required"))?;
    let command = metadata.validate(&filename, source)?;
    let storage = required_storage(&state)?;
    let response =
        state.submissions().submit(contest_id, command, context.user(), peer.ip(), storage).await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[utoipa::path(post, path = "/api/practice/submissions", operation_id = "submitPracticeSource", tag = "practice", request_body(content = inline(SubmissionUploadRequest), content_type = "multipart/form-data"), responses((status = 202, body = SubmitResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn submit_practice(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<SubmitResponse>), AppError> {
    context.require_password_ready()?;
    let mut metadata = None;
    let mut source = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::validation("request", "must be valid multipart data"))?
    {
        match field.name() {
            Some("metadata") if metadata.is_none() => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| AppError::validation("metadata", "must be UTF-8 JSON"))?;
                metadata =
                    Some(serde_json::from_str::<PracticeSubmitMetadata>(&text).map_err(|_| {
                        AppError::validation(
                            "metadata",
                            "must be a valid practice submission object",
                        )
                    })?);
            }
            Some("source") if source.is_none() => {
                let filename = field
                    .file_name()
                    .ok_or_else(|| AppError::validation("source", "must have a filename"))?
                    .to_owned();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::validation("source", "could not be read"))?;
                source = Some((filename, bytes));
            }
            _ => {
                return Err(AppError::validation(
                    "request",
                    "must contain exactly one metadata and one source field",
                ));
            }
        }
    }
    let metadata = metadata.ok_or_else(|| AppError::validation("metadata", "is required"))?;
    let (filename, source) = source.ok_or_else(|| AppError::validation("source", "is required"))?;
    let (command, enrollment_id, virtual_session_id) = metadata.validate(&filename, source)?;
    let storage = required_storage(&state)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            state
                .submissions()
                .submit_practice(
                    command,
                    enrollment_id,
                    virtual_session_id,
                    context.user(),
                    peer.ip(),
                    storage,
                )
                .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/api/practice/submissions", operation_id = "listPracticeSubmissions", tag = "practice", params(("problemId" = Option<i64>, Query), ("status" = Option<String>, Query), ("language" = Option<String>, Query), ("page" = Option<u32>, Query), ("size" = Option<u32>, Query)), responses((status = 200, body = PageResponse<PracticeSubmissionSummary>)), security(("session_cookie" = [])))]
pub async fn list_practice(
    context: AuthContext,
    State(state): State<AppState>,
    query: Result<Query<SubmissionListQuery>, QueryRejection>,
) -> Result<Json<PageResponse<PracticeSubmissionSummary>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid practice filters"))?;
    Ok(Json(state.submissions().list_practice(context.user(), query.validate()?).await?))
}

#[utoipa::path(get, path = "/api/practice/progress", operation_id = "listPracticeProgress", tag = "practice", responses((status = 200, body = [PracticeProblemStatus])), security(("session_cookie" = [])))]
pub async fn practice_progress(
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<PracticeProblemStatus>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.submissions().practice_progress(context.user()).await?))
}

#[utoipa::path(get, path = "/api/practice/submissions/{submission_id}", operation_id = "getPracticeSubmission", tag = "practice", params(("submission_id" = i64, Path)), responses((status = 200, body = PracticeSubmissionDetail)), security(("session_cookie" = [])))]
pub async fn practice_detail(
    context: AuthContext,
    State(state): State<AppState>,
    Path(submission_id): Path<i64>,
) -> Result<Json<PracticeSubmissionDetail>, AppError> {
    context.require_password_ready()?;
    let storage = required_storage(&state)?;
    Ok(Json(state.submissions().practice_detail(submission_id, context.user(), storage).await?))
}
