use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
};

use crate::{
    error::AppError, features::auth::SuperAdminContext, pagination::PageResponse, state::AppState,
};

use super::model::{
    CreateStaffAccountRequest, PageQuery, ResetStaffPasswordRequest, StaffAccountResponse,
    UpdateStaffAccountRequest,
};

#[utoipa::path(get, path = "/api/admin/staff-accounts", operation_id = "listStaffAccounts", tag = "staff-accounts", params(("page" = Option<u32>, Query), ("size" = Option<u32>, Query), ("sort" = Option<String>, Query)), responses((status = 200, body = PageResponse<StaffAccountResponse>), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    query: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<StaffAccountResponse>>, AppError> {
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain valid pagination values"))?;
    Ok(Json(state.staff_accounts().list(query).await?))
}

#[utoipa::path(post, path = "/api/admin/staff-accounts", operation_id = "createStaffAccount", tag = "staff-accounts", request_body = CreateStaffAccountRequest, responses((status = 200, body = StaffAccountResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<CreateStaffAccountRequest>, JsonRejection>,
) -> Result<Json<StaffAccountResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid staff account object"))?;
    let request = request.validate()?;
    Ok(Json(state.staff_accounts().create(request, context.user().id, peer.ip()).await?))
}

#[utoipa::path(patch, path = "/api/admin/staff-accounts/{user_id}", operation_id = "updateStaffAccount", tag = "staff-accounts", params(("user_id" = i64, Path)), request_body = UpdateStaffAccountRequest, responses((status = 200, body = StaffAccountResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user_id): Path<i64>,
    payload: Result<Json<UpdateStaffAccountRequest>, JsonRejection>,
) -> Result<Json<StaffAccountResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid staff account update"))?;
    let request = request.validate()?;
    Ok(Json(state.staff_accounts().update(user_id, request, context.user().id, peer.ip()).await?))
}

#[utoipa::path(post, path = "/api/admin/staff-accounts/{user_id}/reset-password", operation_id = "resetStaffPassword", tag = "staff-accounts", params(("user_id" = i64, Path)), request_body = ResetStaffPasswordRequest, responses((status = 200, body = StaffAccountResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reset_password(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(user_id): Path<i64>,
    payload: Result<Json<ResetStaffPasswordRequest>, JsonRejection>,
) -> Result<Json<StaffAccountResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid password reset object"))?;
    let new_password = request.validate()?;
    Ok(Json(
        state
            .staff_accounts()
            .reset_password(user_id, new_password, context.user().id, peer.ip())
            .await?,
    ))
}
