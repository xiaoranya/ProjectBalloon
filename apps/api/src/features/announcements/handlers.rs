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
    features::auth::{AuthContext, ContestManagerContext},
    state::AppState,
};

use crate::features::announcements::model::{
    AnnouncementResponse, CreateRequest, ListQuery, PinRequest, UpdateRequest,
};

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/announcements",
    operation_id = "createAnnouncement",
    tag = "announcements",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = CreateRequest,
    responses(
        (status = 201, description = "Announcement created", body = AnnouncementResponse),
        (status = 400, description = "Invalid announcement", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest is archived or scheduling state conflicts", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<AnnouncementResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid announcement"))?;
    Ok((
        StatusCode::CREATED,
        Json(state.announcements().create(contest_id, request, context.user(), peer.ip()).await?),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/announcements/{id}",
    operation_id = "updateAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    request_body = UpdateRequest,
    responses(
        (status = 200, description = "Announcement updated", body = AnnouncementResponse),
        (status = 400, description = "Invalid update", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Version, lifecycle, or archive conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<UpdateRequest>, JsonRejection>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid announcement update"))?;
    Ok(Json(state.announcements().update(id, request, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    post,
    path = "/api/announcements/{id}/schedule",
    operation_id = "rescheduleAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    request_body = CreateRequest,
    responses(
        (status = 200, description = "Scheduled announcement updated", body = AnnouncementResponse),
        (status = 400, description = "Invalid schedule", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Announcement is not schedulable", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn schedule(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid announcement schedule"))?;
    Ok(Json(state.announcements().update_scheduled(id, request, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    post,
    path = "/api/announcements/{id}/cancel",
    operation_id = "cancelScheduledAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    responses(
        (status = 200, description = "Scheduled announcement cancelled", body = AnnouncementResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Announcement is not scheduled or contest is archived", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn cancel(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    Ok(Json(state.announcements().cancel_scheduled(id, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    post,
    path = "/api/announcements/{id}/pin",
    operation_id = "pinAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    request_body = PinRequest,
    responses(
        (status = 200, description = "Announcement pin state updated", body = AnnouncementResponse),
        (status = 400, description = "Invalid pin state", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Announcement is not published or contest is archived", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn pin(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
    payload: Result<Json<PinRequest>, JsonRejection>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain pinned"))?;
    Ok(Json(state.announcements().pin(id, request.pinned, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    post,
    path = "/api/announcements/{id}/withdraw",
    operation_id = "withdrawAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    responses(
        (status = 204, description = "Announcement withdrawn"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Announcement is not published or contest is archived", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn withdraw(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    state.announcements().withdraw(id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/announcements",
    operation_id = "listAnnouncements",
    tag = "announcements",
    params(
        ("contest_id" = i64, Path, description = "Contest identifier"),
        ("includeWithdrawn" = Option<bool>, Query, description = "Include withdrawn and scheduled records; contest manager only")
    ),
    responses(
        (status = 200, description = "Visible announcements", body = [AnnouncementResponse]),
        (status = 400, description = "Invalid query", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is not visible to the actor", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Vec<AnnouncementResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "contains invalid announcement filters"))?;
    Ok(Json(state.announcements().list(contest_id, query.include_withdrawn, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/announcements/{id}",
    operation_id = "getAnnouncement",
    tag = "announcements",
    params(("id" = i64, Path, description = "Announcement identifier")),
    responses(
        (status = 200, description = "Announcement", body = AnnouncementResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Announcement not found or not visible", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AnnouncementResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.announcements().get(id, context.user()).await?))
}
