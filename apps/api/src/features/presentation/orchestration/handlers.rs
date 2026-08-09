use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{error::AppError, features::auth::AuthContext, state::AppState};

use super::super::service::require_screen_operator;
use super::model::{
    GroupControlRequest, GroupRequest, GroupResponse, PlaylistRequest, PlaylistResponse,
};
use super::service::OrchestrationService;

fn orchestration(state: &AppState) -> OrchestrationService {
    OrchestrationService::new(state.database().clone())
}
macro_rules! auth {
    ($context:expr) => {{
        $context.require_password_ready()?;
        require_screen_operator($context.user())?;
    }};
}
#[utoipa::path(get, path = "/api/contests/{contest_id}/screen-playlists", operation_id = "listScreenPlaylists", tag = "screens", params(("contest_id" = i64, Path)), responses((status = 200, body = [PlaylistResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_playlists(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<PlaylistResponse>>, AppError> {
    auth!(context);
    Ok(Json(orchestration(&state).list_playlists(contest, context.user()).await?))
}
#[utoipa::path(post, path = "/api/contests/{contest_id}/screen-playlists", operation_id = "createScreenPlaylist", tag = "screens", params(("contest_id" = i64, Path)), request_body = PlaylistRequest, responses((status = 201, body = PlaylistResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<PlaylistRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PlaylistResponse>), AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen playlist"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            orchestration(&state)
                .create_playlist(contest, request, context.user(), peer.ip())
                .await?,
        ),
    ))
}
#[utoipa::path(put, path = "/api/screen-playlists/{playlist_id}", operation_id = "updateScreenPlaylist", tag = "screens", params(("playlist_id" = i64, Path)), request_body = PlaylistRequest, responses((status = 200, body = PlaylistResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<PlaylistRequest>, JsonRejection>,
) -> Result<Json<PlaylistResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen playlist"))?;
    Ok(Json(orchestration(&state).update_playlist(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(delete, path = "/api/screen-playlists/{playlist_id}", operation_id = "deleteScreenPlaylist", tag = "screens", params(("playlist_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn delete_playlist(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    auth!(context);
    orchestration(&state).delete_playlist(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get, path = "/api/contests/{contest_id}/screen-groups", operation_id = "listScreenGroups", tag = "screens", params(("contest_id" = i64, Path)), responses((status = 200, body = [GroupResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_groups(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<GroupResponse>>, AppError> {
    auth!(context);
    Ok(Json(orchestration(&state).list_groups(contest, context.user()).await?))
}
#[utoipa::path(post, path = "/api/contests/{contest_id}/screen-groups", operation_id = "createScreenGroup", tag = "screens", params(("contest_id" = i64, Path)), request_body = GroupRequest, responses((status = 201, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<GroupRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<GroupResponse>), AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            orchestration(&state).create_group(contest, request, context.user(), peer.ip()).await?,
        ),
    ))
}
#[utoipa::path(put, path = "/api/screen-groups/{group_id}", operation_id = "updateScreenGroup", tag = "screens", params(("group_id" = i64, Path)), request_body = GroupRequest, responses((status = 200, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<GroupRequest>, JsonRejection>,
) -> Result<Json<GroupResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group"))?;
    Ok(Json(orchestration(&state).update_group(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(post, path = "/api/screen-groups/{group_id}/control", operation_id = "controlScreenGroup", tag = "screens", params(("group_id" = i64, Path)), request_body = GroupControlRequest, responses((status = 200, body = GroupResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn control_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<GroupControlRequest>, JsonRejection>,
) -> Result<Json<GroupResponse>, AppError> {
    auth!(context);
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen group control"))?;
    Ok(Json(orchestration(&state).control(id, request, context.user(), peer.ip()).await?))
}
#[utoipa::path(delete, path = "/api/screen-groups/{group_id}", operation_id = "deleteScreenGroup", tag = "screens", params(("group_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn delete_group(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    auth!(context);
    orchestration(&state).delete_group(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}
