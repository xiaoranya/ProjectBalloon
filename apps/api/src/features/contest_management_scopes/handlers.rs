use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
};

use crate::{error::AppError, features::auth::SuperAdminContext, state::AppState};

use crate::features::contest_management_scopes::model::{
    ContestManagementScopeResponse, ReplaceContestManagementScopeRequest,
};

#[utoipa::path(get, path = "/api/admin/contest-managers", operation_id = "listContestManagementScopes", tag = "admin-scopes", responses((status = 200, body = [ContestManagementScopeResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    _context: SuperAdminContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<ContestManagementScopeResponse>>, AppError> {
    Ok(Json(state.contest_management_scopes().list().await?))
}

#[utoipa::path(put, path = "/api/admin/contest-managers/{user_id}/contests", operation_id = "replaceContestManagementScope", tag = "admin-scopes", params(("user_id" = i64, Path)), request_body = ReplaceContestManagementScopeRequest, responses((status = 200, body = ContestManagementScopeResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn replace(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user_id): Path<i64>,
    payload: Result<Json<ReplaceContestManagementScopeRequest>, JsonRejection>,
) -> Result<Json<ContestManagementScopeResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid contest scope object"))?;
    let contest_ids = request.validate()?;
    Ok(Json(
        state
            .contest_management_scopes()
            .replace(user_id, contest_ids, context.user().id, peer.ip())
            .await?,
    ))
}
