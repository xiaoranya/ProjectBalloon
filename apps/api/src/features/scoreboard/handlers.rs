use std::net::SocketAddr;

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

use crate::{
    error::AppError,
    features::auth::{ContestManagerContext, OptionalAuthContext},
    state::AppState,
};

use super::model::{
    ScoreboardQuery, ScoreboardResponse, ScoreboardSnapshotResponse, SnapshotSelector,
};
use super::service::to_csv;

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/scoreboard",
    operation_id = "getPublicScoreboard",
    tag = "scoreboard",
    params(
        ("contest_id" = i64, Path),
        ("groupName" = Option<String>, Query),
        ("participationType" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Public contest scoreboard honoring freeze visibility", body = ScoreboardResponse),
        (status = 400, description = "Invalid scoreboard filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Supplied session is invalid or expired", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is not visible", body = crate::error::ApiErrorBody)
    ),
    security((), ("session_cookie" = []))
)]
pub async fn public(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ScoreboardQuery>, QueryRejection>,
) -> Result<Json<ScoreboardResponse>, AppError> {
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "must contain valid scoreboard filters"))?;
    state.contests().get(contest_id, context.user()).await?;
    Ok(Json(state.scoreboard().public(contest_id, query.validate()?).await?))
}

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/scoreboard.csv",
    operation_id = "exportPublicScoreboardCsv",
    tag = "scoreboard",
    params(
        ("contest_id" = i64, Path),
        ("groupName" = Option<String>, Query),
        ("participationType" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Public scoreboard CSV", body = String, content_type = "text/csv"),
        (status = 400, description = "Invalid scoreboard filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Supplied session is invalid or expired", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is not visible", body = crate::error::ApiErrorBody)
    ),
    security((), ("session_cookie" = []))
)]
pub async fn public_csv(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ScoreboardQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "must contain valid scoreboard filters"))?;
    state.contests().get(contest_id, context.user()).await?;
    let board = state.scoreboard().public(contest_id, query.validate()?).await?;
    csv_response(contest_id, "public", to_csv(&board))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/scoreboard",
    operation_id = "getAdminScoreboard",
    tag = "scoreboard",
    params(
        ("contest_id" = i64, Path),
        ("groupName" = Option<String>, Query),
        ("participationType" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Administrative contest scoreboard", body = ScoreboardResponse),
        (status = 400, description = "Invalid scoreboard filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn admin(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ScoreboardQuery>, QueryRejection>,
) -> Result<Json<ScoreboardResponse>, AppError> {
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "must contain valid scoreboard filters"))?;
    Ok(Json(state.scoreboard().admin(contest_id, context.user(), query.validate()?).await?))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/scoreboard.csv",
    operation_id = "exportAdminScoreboardCsv",
    tag = "scoreboard",
    params(
        ("contest_id" = i64, Path),
        ("groupName" = Option<String>, Query),
        ("participationType" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Administrative scoreboard CSV", body = String, content_type = "text/csv"),
        (status = 400, description = "Invalid scoreboard filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn admin_csv(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ScoreboardQuery>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(query) = query
        .map_err(|_| AppError::validation("query", "must contain valid scoreboard filters"))?;
    let board = state.scoreboard().admin(contest_id, context.user(), query.validate()?).await?;
    csv_response(contest_id, "admin", to_csv(&board))
}

fn csv_response(contest_id: i64, variant: &str, csv: String) -> Result<Response, AppError> {
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=contest-{contest_id}-scoreboard-{variant}.csv"
    ))
    .map_err(|error| AppError::internal("build scoreboard CSV filename", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8")),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        csv,
    )
        .into_response())
}

#[utoipa::path(
    post,
    path = "/api/admin/contests/{contest_id}/scoreboard/snapshots",
    operation_id = "createScoreboardSnapshot",
    tag = "scoreboard",
    params(("contest_id" = i64, Path)),
    request_body = SnapshotSelector,
    responses(
        (status = 201, description = "Scoreboard snapshot created", body = ScoreboardSnapshotResponse),
        (status = 400, description = "Invalid snapshot variant or filters", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest is outside the actor's management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest schedule or snapshot state conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create_snapshot(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<SnapshotSelector>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<ScoreboardSnapshotResponse>), AppError> {
    let Json(selector) = payload
        .map_err(|_| AppError::validation("request", "must be a valid snapshot selector"))?;
    let snapshot = state
        .scoreboard()
        .create_snapshot(contest_id, context.user(), peer.ip(), selector.validate()?)
        .await?;
    Ok((axum::http::StatusCode::CREATED, Json(snapshot)))
}

#[utoipa::path(
    get,
    path = "/api/admin/contests/{contest_id}/scoreboard/snapshots/latest",
    operation_id = "getLatestScoreboardSnapshot",
    tag = "scoreboard",
    params(
        ("contest_id" = i64, Path),
        ("variant" = String, Query),
        ("groupName" = Option<String>, Query),
        ("participationType" = Option<String>, Query)
    ),
    responses(
        (status = 200, description = "Latest matching scoreboard snapshot", body = ScoreboardSnapshotResponse),
        (status = 400, description = "Invalid snapshot selector", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest or matching snapshot was not found", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn latest_snapshot(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<SnapshotSelector>, QueryRejection>,
) -> Result<Json<ScoreboardSnapshotResponse>, AppError> {
    let Query(selector) =
        query.map_err(|_| AppError::validation("query", "must be a valid snapshot selector"))?;
    Ok(Json(
        state
            .scoreboard()
            .latest_snapshot(contest_id, context.user(), selector.validate()?)
            .await?,
    ))
}
