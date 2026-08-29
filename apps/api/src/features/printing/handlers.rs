use std::net::SocketAddr;

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{error::AppError, features::auth::AuthContext, state::AppState};

use crate::features::printing::model::{
    CreateRequest, ListQuery, PrintRequestResponse, RejectRequest,
};
use crate::features::printing::service::storage;

#[utoipa::path(post, path = "/api/contests/{contest_id}/print-requests", operation_id = "createPrintRequest", tag = "printing",
    params(("contest_id" = i64, Path)), request_body = CreateRequest,
    responses((status = 201, body = PrintRequestResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 429, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PrintRequestResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain printable text"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .printing()
                .create(
                    contest_id,
                    request.validate()?,
                    context.user(),
                    peer.ip(),
                    storage(&state)?,
                )
                .await?,
        ),
    ))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/print-requests/mine", operation_id = "listOwnPrintRequests", tag = "printing",
    params(("contest_id" = i64, Path)), responses((status = 200, body = [PrintRequestResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_mine(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<PrintRequestResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().list_mine(contest_id, context.user()).await?))
}

#[utoipa::path(get, path = "/api/contests/{contest_id}/print-requests/all", operation_id = "listAllPrintRequests", tag = "printing",
    params(("contest_id" = i64, Path), ("status" = Option<String>, Query)), responses((status = 200, body = [PrintRequestResponse]), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_all(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Vec<PrintRequestResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "contains invalid print filters"))?;
    Ok(Json(state.printing().list_all(contest_id, query.validate()?, context.user()).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/retry", operation_id = "retryPrintRequest", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = PrintRequestResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn retry(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().transition(id, "RETRY", context.user(), peer.ip(), None).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/cancel", operation_id = "cancelPrintRequest", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = PrintRequestResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn cancel(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.printing().transition(id, "CANCEL", context.user(), peer.ip(), None).await?))
}

#[utoipa::path(post, path = "/api/print-requests/{id}/reject", operation_id = "rejectPrintRequest", tag = "printing",
    params(("id" = i64, Path)), request_body = RejectRequest, responses((status = 200, body = PrintRequestResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn reject(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<RejectRequest>, JsonRejection>,
) -> Result<Json<PrintRequestResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain a rejection reason"))?;
    Ok(Json(
        state
            .printing()
            .transition(id, "REJECT", context.user(), peer.ip(), Some(request.reason))
            .await?,
    ))
}

#[utoipa::path(get, path = "/api/print-requests/{id}/pdf", operation_id = "downloadPrintPdf", tag = "printing",
    params(("id" = i64, Path)), responses((status = 200, body = Vec<u8>, content_type = "application/pdf"), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody), (status = 503, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn download_pdf(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    let pdf = state.printing().pdf(id, context.user(), storage(&state)?).await?;
    let disposition = HeaderValue::from_str(&format!("attachment; filename=print-{id}.pdf"))
        .map_err(|error| AppError::internal("build print filename", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/pdf")),
            (header::CONTENT_DISPOSITION, disposition),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        ],
        pdf,
    )
        .into_response())
}
