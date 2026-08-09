use std::net::SocketAddr;

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
};

use super::model::{
    BalloonStatsResponse, BalloonTaskResponse, CancelRequest, DispatchPolicyRequest,
    DispatchPolicyResponse, DispatchQuery, ListQuery, NoteRequest, VersionRequest,
};
use super::service::validate_status;
use crate::{error::AppError, features::auth::AuthContext, state::AppState};

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons", operation_id = "listBalloonTasks", tag = "balloons", params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [BalloonTaskResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Vec<BalloonTaskResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) = query.map_err(|_| AppError::validation("query", "invalid filters"))?;
    Ok(Json(
        state.balloons().list(contest_id, validate_status(query.status)?, context.user()).await?,
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons/stats", operation_id = "getBalloonStats", tag = "balloons", params(("contest_id" = i64, Path)), responses((status = 200, body = BalloonStatsResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn stats(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<BalloonStatsResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.balloons().stats(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/balloons/dispatch-policy", operation_id = "getBalloonDispatchPolicy", tag = "balloons", params(("contest_id" = i64, Path)), responses((status = 200, body = DispatchPolicyResponse)), security(("session_cookie" = [])))]
pub async fn dispatch_policy(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<DispatchPolicyResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.balloons().dispatch_policy(contest_id, context.user()).await?))
}

#[utoipa::path(put, path = "/api/contests/{contest_id}/balloons/dispatch-policy", operation_id = "updateBalloonDispatchPolicy", tag = "balloons", params(("contest_id" = i64, Path)), request_body = DispatchPolicyRequest, responses((status = 200, body = DispatchPolicyResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_dispatch_policy(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<DispatchPolicyRequest>, JsonRejection>,
) -> Result<Json<DispatchPolicyResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid dispatch policy"))?;
    Ok(Json(state.balloons().update_dispatch_policy(contest_id, request, context.user()).await?))
}

#[utoipa::path(post, path = "/api/contests/{contest_id}/balloons/dispatch", operation_id = "dispatchBalloonTasks", tag = "balloons", params(("contest_id" = i64, Path), ("limit" = Option<i32>, Query), ("zone" = Option<String>, Query)), responses((status = 200, body = [BalloonTaskResponse])), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn dispatch(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<DispatchQuery>, QueryRejection>,
) -> Result<Json<Vec<BalloonTaskResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "invalid dispatch request"))?;
    Ok(Json(state.balloons().dispatch(contest_id, query, context.user()).await?))
}

async fn version_payload(
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<i32, AppError> {
    payload
        .map(|Json(request)| request.expected_version)
        .map_err(|_| AppError::validation("request", "must contain expectedVersion"))
}

#[utoipa::path(post, path = "/api/balloons/{id}/claim", operation_id = "claimBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn claim(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "CLAIM",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/deliver", operation_id = "deliverBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn deliver(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "DELIVER",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/cancel", operation_id = "cancelBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = CancelRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn cancel(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<CancelRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain expectedVersion and reason"))?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "CANCEL",
                request.expected_version,
                Some(request.reason),
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(post, path = "/api/balloons/{id}/reopen", operation_id = "reopenBalloon", tag = "balloons", params(("id" = i64, Path)), request_body = VersionRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reopen(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<VersionRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(
        state
            .balloons()
            .transition(
                id,
                "REOPEN",
                version_payload(payload).await?,
                None,
                context.user(),
                peer.ip(),
            )
            .await?,
    ))
}

#[utoipa::path(patch, path = "/api/balloons/{id}/note", operation_id = "updateBalloonNote", tag = "balloons", params(("id" = i64, Path)), request_body = NoteRequest, responses((status = 200, body = BalloonTaskResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn note(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<NoteRequest>, JsonRejection>,
) -> Result<Json<BalloonTaskResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
    Ok(Json(state.balloons().note(id, request, context.user(), peer.ip()).await?))
}
