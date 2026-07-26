use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
};

use crate::{error::AppError, features::auth::SuperAdminContext, state::AppState};

use super::model::{ContestAdminScopeResponse, ReplaceContestAdminScopeRequest};

#[utoipa::path(get, path = "/api/admin/contest-admins", operation_id = "listContestAdminScopes", tag = "admin-scopes", responses((status = 200, body = [ContestAdminScopeResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    _context: SuperAdminContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<ContestAdminScopeResponse>>, AppError> {
    Ok(Json(state.contest_admin_scopes().list().await?))
}

#[utoipa::path(put, path = "/api/admin/contest-admins/{user_id}/contests", operation_id = "replaceContestAdminScope", tag = "admin-scopes", params(("user_id" = i64, Path)), request_body = ReplaceContestAdminScopeRequest, responses((status = 200, body = ContestAdminScopeResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn replace(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user_id): Path<i64>,
    payload: Result<Json<ReplaceContestAdminScopeRequest>, JsonRejection>,
) -> Result<Json<ContestAdminScopeResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid contest scope object"))?;
    let contest_ids = request.validate()?;
    Ok(Json(
        state
            .contest_admin_scopes()
            .replace(user_id, contest_ids, context.user().id, peer.ip())
            .await?,
    ))
}
