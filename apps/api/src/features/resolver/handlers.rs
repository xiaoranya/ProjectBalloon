use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
};

use crate::error::AppError;
use crate::features::auth::AuthContext;
use crate::state::AppState;

use super::model::{
    AutoPlayRequest, CommandRequest, CreateRequest, ResolverEventResponse,
    ResolverPublicStateResponse, ResolverRunResponse, ResolverSourcesResponse,
};

#[utoipa::path(post, path = "/api/admin/contests/{contest_id}/resolver-runs", operation_id = "createResolverRun", tag = "resolver", params(("contest_id" = i64, Path)), request_body = CreateRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must contain source snapshot identifiers"))?;
    Ok(Json(state.resolver().create(contest_id, request, context.user(), peer.ip()).await?))
}

#[utoipa::path(get, path = "/api/admin/resolver-runs/{id}", operation_id = "getResolverRun", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = ResolverRunResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().get(id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/resolver-runs", operation_id = "listResolverRuns", tag = "resolver", params(("contest_id" = i64, Path)), responses((status = 200, body = [ResolverRunResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<ResolverRunResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().list(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/resolver-sources", operation_id = "getResolverSources", tag = "resolver", params(("contest_id" = i64, Path)), responses((status = 200, body = ResolverSourcesResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn sources(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<ResolverSourcesResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().sources(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/public/resolver-runs/{id}/state", operation_id = "getPublicResolverState", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = ResolverPublicStateResponse), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn public_state(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ResolverPublicStateResponse>, AppError> {
    Ok(Json(state.resolver().public_state(id).await?))
}

#[utoipa::path(get, path = "/api/admin/resolver-runs/{id}/events", operation_id = "listResolverEvents", tag = "resolver", params(("id" = i64, Path)), responses((status = 200, body = [ResolverEventResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn events(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ResolverEventResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.resolver().events(id, context.user()).await?))
}

macro_rules! command_handler {
    ($name:ident, $action:literal, $path:literal, $operation:literal) => {
        #[utoipa::path(post, path = $path, operation_id = $operation, tag = "resolver", params(("id" = i64, Path)), request_body = CommandRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
        pub async fn $name(
            context: AuthContext,
            State(state): State<AppState>,
            ConnectInfo(peer): ConnectInfo<SocketAddr>,
            Path(id): Path<i64>,
            payload: Result<Json<CommandRequest>, JsonRejection>,
        ) -> Result<Json<ResolverRunResponse>, AppError> {
            context.require_password_ready()?;
            let Json(request) = payload
                .map_err(|_| AppError::validation("request", "must contain expectedVersion"))?;
            Ok(Json(
                state
                    .resolver()
                    .command(id, $action, request.expected_version, context.user(), peer.ip())
                    .await?,
            ))
        }
    };
}

command_handler!(start, "START", "/api/admin/resolver-runs/{id}/start", "startResolverRun");
command_handler!(next, "NEXT", "/api/admin/resolver-runs/{id}/next", "nextResolverStep");
command_handler!(
    previous,
    "PREVIOUS",
    "/api/admin/resolver-runs/{id}/previous",
    "previousResolverStep"
);
command_handler!(pause, "PAUSE", "/api/admin/resolver-runs/{id}/pause", "pauseResolverRun");
command_handler!(resume, "RESUME", "/api/admin/resolver-runs/{id}/resume", "resumeResolverRun");
command_handler!(
    complete,
    "COMPLETE",
    "/api/admin/resolver-runs/{id}/complete",
    "completeResolverRun"
);

#[utoipa::path(post, path = "/api/admin/resolver-runs/{id}/auto-play", operation_id = "configureResolverAutoPlay", tag = "resolver", params(("id" = i64, Path)), request_body = AutoPlayRequest, responses((status = 200, body = ResolverRunResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn auto_play(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<AutoPlayRequest>, JsonRejection>,
) -> Result<Json<ResolverRunResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload.map_err(|_| {
        AppError::validation(
            "request",
            "must contain expectedVersion, enabled, and intervalMilliseconds",
        )
    })?;
    Ok(Json(state.resolver().configure_auto_play(id, request, context.user(), peer.ip()).await?))
}
