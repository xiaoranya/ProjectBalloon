use std::net::SocketAddr;

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};

use crate::{
    error::AppError,
    features::{announcements::AnnouncementResponse, auth::AuthContext},
    state::AppState,
};

use crate::features::clarifications::model::{
    AskRequest, ClarificationResponse, ConvertRequest, ListAllQuery, ReplyRequest,
};

#[utoipa::path(post, path = "/api/contests/{contest_id}/clarifications", operation_id = "askClarification", tag = "clarifications", params(("contest_id" = i64, Path)), request_body = AskRequest, responses((status = 201, body = ClarificationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 429, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn ask(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<AskRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ClarificationResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid clarification"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .clarifications()
                .ask(contest_id, request.validate()?, context.user(), peer.ip())
                .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/clarifications/mine", operation_id = "listOwnClarifications", tag = "clarifications", params(("contest_id" = i64, Path)), responses((status = 200, body = [ClarificationResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_mine(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<ClarificationResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.clarifications().list_mine(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/clarifications/all", operation_id = "listAllClarifications", tag = "clarifications", params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [ClarificationResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_all(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListAllQuery>, QueryRejection>,
) -> Result<Json<Vec<ClarificationResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "contains an invalid clarification status"))?;
    Ok(Json(state.clarifications().list_all(contest_id, query.validate()?, context.user()).await?))
}

#[utoipa::path(get, path = "/api/clarifications/{id}", operation_id = "getClarification", tag = "clarifications", params(("id" = i64, Path)), responses((status = 200, body = ClarificationResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ClarificationResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.clarifications().get(id, context.user()).await?))
}

#[utoipa::path(post, path = "/api/clarifications/{id}/reply", operation_id = "replyClarification", tag = "clarifications", params(("id" = i64, Path)), request_body = ReplyRequest, responses((status = 200, body = ClarificationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reply(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<ReplyRequest>, JsonRejection>,
) -> Result<Json<ClarificationResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid clarification reply"))?;
    Ok(Json(
        state.clarifications().reply(id, request.validate()?, context.user(), peer.ip()).await?,
    ))
}

#[utoipa::path(post, path = "/api/clarifications/{id}/close", operation_id = "closeClarification", tag = "clarifications", params(("id" = i64, Path)), responses((status = 204, description = "Clarification closed"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn close(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    context.require_password_ready()?;
    state.clarifications().close(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/clarifications/{id}/convert", operation_id = "convertClarification", tag = "clarifications", params(("id" = i64, Path)), request_body = ConvertRequest, responses((status = 200, body = crate::features::announcements::AnnouncementResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn convert(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<ConvertRequest>, JsonRejection>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid conversion request"))?;
    Ok(Json(state.clarifications().convert(id, request, context.user(), peer.ip()).await?))
}
