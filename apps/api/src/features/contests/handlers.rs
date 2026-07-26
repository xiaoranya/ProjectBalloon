use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{
    error::AppError,
    features::auth::{ContestManagerContext, OptionalAuthContext, SuperAdminContext},
    pagination::PageResponse,
    state::AppState,
};

use super::model::{
    ContestCloneRequest, ContestCloneResponse, ContestExtensionRequest, ContestExtensionResponse,
    ContestListQuery, ContestResponse, CreateContestRequest, LifecycleTransitionRequest,
    LifecycleTransitionResponse, UpdateContestRequest,
};

#[utoipa::path(
    get,
    path = "/api/contests",
    operation_id = "listContests",
    tag = "contests",
    params(
        ("page" = Option<u32>, Query, description = "Zero-based page index; defaults to 0"),
        ("size" = Option<u32>, Query, description = "Page size from 1 through 500; defaults to 50"),
        ("sort" = Option<String>, Query, description = "Allowed field and direction, for example updatedAt,desc"),
        ("includeDeleted" = Option<bool>, Query, description = "Include soft-deleted contests; super administrator only"),
        ("manageableOnly" = Option<bool>, Query, description = "Return only contests manageable by the current contest administrator")
    ),
    responses(
        (status = 200, description = "Contests visible to the anonymous or authenticated actor", body = PageResponse<ContestResponse>),
        (status = 400, description = "Invalid pagination or sort filter", body = crate::error::ApiErrorBody),
        (status = 401, description = "Supplied session is invalid or expired", body = crate::error::ApiErrorBody),
        (status = 403, description = "Requested administrative filter is not permitted", body = crate::error::ApiErrorBody)
    ),
    security((), ("session_cookie" = []))
)]
pub async fn list(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    query: Result<Query<ContestListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<ContestResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain valid contest filters"))?;
    Ok(Json(state.contests().list(query.validate()?, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}",
    operation_id = "getContest",
    tag = "contests",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    responses(
        (status = 200, description = "Contest visible to the anonymous or authenticated actor", body = ContestResponse),
        (status = 401, description = "Supplied session is invalid or expired", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is not visible", body = crate::error::ApiErrorBody)
    ),
    security((), ("session_cookie" = []))
)]
pub async fn get(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<ContestResponse>, AppError> {
    Ok(Json(state.contests().get(contest_id, context.user()).await?))
}

#[utoipa::path(
    post,
    path = "/api/contests",
    operation_id = "createContest",
    tag = "contests",
    request_body = CreateContestRequest,
    responses(
        (status = 201, description = "Draft contest created", body = ContestResponse),
        (status = 400, description = "Invalid contest name or schedule", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest name is already in use", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<CreateContestRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ContestResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid contest object"))?;
    let contest =
        state.contests().create(request.validate()?, context.user().id, peer.ip()).await?;
    Ok((StatusCode::CREATED, Json(contest)))
}

#[utoipa::path(
    post,
    path = "/api/contests/{source_contest_id}/clones",
    operation_id = "cloneContest",
    tag = "contests",
    params(("source_contest_id" = i64, Path, description = "Source contest identifier")),
    request_body = ContestCloneRequest,
    responses(
        (status = 201, description = "Draft contest cloned with problem configuration and optionally active teams", body = ContestCloneResponse),
        (status = 400, description = "Invalid clone name or schedule", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Source contest was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Target contest name is already in use", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn clone_contest(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(source_contest_id): Path<i64>,
    payload: Result<Json<ContestCloneRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ContestCloneResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid contest clone"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .contests()
                .clone_contest(source_contest_id, request.validate()?, context.user().id, peer.ip())
                .await?,
        ),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/contests/{contest_id}",
    operation_id = "updateContest",
    tag = "contests",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = UpdateContestRequest,
    responses(
        (status = 200, description = "Contest updated", body = ContestResponse),
        (status = 400, description = "Invalid update or incomplete schedule", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Name, schedule lifecycle, or archive conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<UpdateContestRequest>, JsonRejection>,
) -> Result<Json<ContestResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid contest update"))?;
    Ok(Json(
        state.contests().update(contest_id, request.validate()?, context.user(), peer.ip()).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/contests/{contest_id}",
    operation_id = "deleteContest",
    tag = "contests",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    responses(
        (status = 204, description = "Contest soft-deleted"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest has assigned teams or is archived", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn delete(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
) -> Result<StatusCode, AppError> {
    state.contests().delete(contest_id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/transitions",
    operation_id = "transitionContest",
    tag = "contests",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = LifecycleTransitionRequest,
    responses(
        (status = 200, description = "Contest lifecycle transitioned", body = LifecycleTransitionResponse),
        (status = 400, description = "Invalid target, missing schedule, or incomplete frozen configuration", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Lifecycle transition is invalid, archive work is active, or contest is archived", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn transition(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<LifecycleTransitionRequest>, JsonRejection>,
) -> Result<Json<LifecycleTransitionResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain a valid lifecycle target"))?;
    Ok(Json(state.contests().transition(contest_id, request.to, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/extensions",
    operation_id = "extendContest",
    tag = "contests",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = ContestExtensionRequest,
    responses(
        (status = 200, description = "Contest end time extended", body = ContestExtensionResponse),
        (status = 400, description = "Invalid extension timestamps", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, or valid CSRF token required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest state, end time, optimistic timestamp, or archive conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn extend(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<ContestExtensionRequest>, JsonRejection>,
) -> Result<Json<ContestExtensionResponse>, AppError> {
    let Json(request) = payload.map_err(|_| {
        AppError::validation("request", "must contain valid contest extension timestamps")
    })?;
    Ok(Json(state.contests().extend(contest_id, request, context.user(), peer.ip()).await?))
}
