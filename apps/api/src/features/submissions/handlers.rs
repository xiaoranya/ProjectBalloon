use std::net::SocketAddr;

use axum::{
    Json,
    body::Body,
    extract::{
        ConnectInfo, Multipart, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext},
    pagination::PageResponse,
    state::AppState,
};

use super::model::{
    JudgeQueueStatusResponse, PracticeProblemStatus, PracticeSubmissionDetail,
    PracticeSubmissionSummary, PracticeSubmitMetadata, RejudgeRequest, RejudgeResponse,
    SubmissionDetail, SubmissionListQuery, SubmissionSummary, SubmissionUploadRequest,
    SubmitMetadata, SubmitResponse,
};
use super::query::{SimilarityPairQuery, SimilarityQuery};
use super::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgePreviewResponse,
    BatchRejudgeTaskResponse, CreateExportTaskRequest, ExportTaskResponse,
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

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/submissions",
    operation_id = "listOwnSubmissions",
    tag = "submissions",
    params(
        ("contest_id" = i64, Path),
        ("problemId" = Option<i64>, Query),
        ("status" = Option<String>, Query),
        ("language" = Option<String>, Query),
        ("page" = Option<u32>, Query),
        ("size" = Option<u32>, Query),
        ("sort" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Authenticated team's submissions", body = PageResponse<SubmissionSummary>),
        (status = 400, description = "Invalid submission filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or team is not registered", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_own(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<SubmissionListQuery>, QueryRejection>,
) -> Result<Json<PageResponse<SubmissionSummary>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid submission filters"))?;
    Ok(Json(state.submissions().list_own(contest_id, context.user(), query.validate()?).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/submissions",
    operation_id = "listAdminSubmissions",
    tag = "submissions",
    params(
        ("contest_id" = i64, Path),
        ("teamId" = Option<i64>, Query),
        ("problemId" = Option<i64>, Query),
        ("status" = Option<String>, Query),
        ("language" = Option<String>, Query),
        ("page" = Option<u32>, Query),
        ("size" = Option<u32>, Query),
        ("sort" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Submissions in the managed contest", body = PageResponse<SubmissionSummary>),
        (status = 400, description = "Invalid submission filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_admin(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<SubmissionListQuery>, QueryRejection>,
) -> Result<Json<PageResponse<SubmissionSummary>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid submission filters"))?;
    Ok(Json(state.submissions().list_admin(contest_id, context.user(), query.validate()?).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/submission-similarity",
    operation_id = "listSubmissionSimilarity",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("problemId" = Option<i64>, Query), ("language" = Option<String>, Query), ("minGroupSize" = Option<u32>, Query)),
    responses((status = 200, description = "Exact normalized duplicate submission groups", body = [super::query::SimilarityGroupResponse]),
        (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn list_similarity(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<SimilarityQuery>, QueryRejection>,
) -> Result<Json<Vec<super::query::SimilarityGroupResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid similarity filters"))?;
    Ok(Json(state.submissions().list_similarity(contest_id, context.user(), query).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/submission-similarity/pairs",
    operation_id = "listSubmissionSimilarityPairs",
    tag = "submissions",
    params(("contest_id" = i64, Path), ("problemId" = Option<i64>, Query), ("language" = Option<String>, Query), ("minSimilarityPercent" = Option<u32>, Query)),
    responses((status = 200, description = "Approximate similar submission pairs", body = [super::query::SimilarityPairResponse]),
        (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)),
    security(("session_cookie" = []))
)]
pub async fn list_similarity_pairs(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<SimilarityPairQuery>, QueryRejection>,
) -> Result<Json<Vec<super::query::SimilarityPairResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid similarity filters"))?;
    Ok(Json(state.submissions().list_similarity_pairs(contest_id, context.user(), query).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/submission-similarity/backfill",
    operation_id = "backfillSubmissionSimilarity",
    tag = "submissions",
    params(("contest_id" = i64, Path)),
    responses((status = 200, description = "Bounded historical similarity backfill", body = super::query::SimilarityBackfillResponse),
        (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn backfill_similarity(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<super::query::SimilarityBackfillResponse>, AppError> {
    let storage = required_storage(&state)?;
    Ok(Json(state.submissions().backfill_similarity(contest_id, context.user(), storage).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/judge-queue/status",
    operation_id = "getJudgeQueueStatus",
    tag = "judge-queue",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    responses(
        (status = 200, description = "Current contest judge queue state", body = JudgeQueueStatusResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn judge_queue_status(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<JudgeQueueStatusResponse>, AppError> {
    Ok(Json(state.submissions().judge_queue_status(contest_id, context.user()).await?))
}

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
        .get_stream(bucket, key)
        .await
        .map_err(|error| AppError::internal("download export task output", error))?;
    let (suffix, content_type) = if task.kind == "METADATA_CSV" {
        ("submissions.csv", "text/csv; charset=utf-8")
    } else {
        ("submission-sources.zip", "application/zip")
    };
    export_response(contest_id, suffix, content_type, Body::from_stream(stream))
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
