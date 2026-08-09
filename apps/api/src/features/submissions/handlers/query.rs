use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext},
    pagination::PageResponse,
    state::AppState,
};

use super::super::model::{JudgeQueueStatusResponse, SubmissionListQuery, SubmissionSummary};
use super::super::query::{
    SimilarityBackfillResponse, SimilarityGroupResponse, SimilarityPairQuery,
    SimilarityPairResponse, SimilarityQuery,
};
use super::required_storage;

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
