use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext},
    state::AppState,
};

use crate::features::contest_problems::model::{
    AssignProblemRequest, ContestProblemDetailResponse, ContestProblemListQuery,
    ContestProblemResponse, ReorderEntry, UpdateContestProblemRequest, validate_reorder,
};

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/problems",
    operation_id = "listContestProblems",
    tag = "contest-problems",
    params(
        ("contest_id" = i64, Path, description = "Contest identifier"),
        ("lang" = Option<String>, Query, description = "Statement language such as en or zh-CN")
    ),
    responses(
        (status = 200, description = "Readable contest problems and published statements", body = [ContestProblemDetailResponse]),
        (status = 400, description = "Invalid statement language", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is not visible", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    query: Result<Query<ContestProblemListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Vec<ContestProblemDetailResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain a valid language"))?;
    Ok(Json(
        state
            .contest_problems()
            .list_readable(contest_id, context.user(), query.validate()?)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/problems",
    operation_id = "assignContestProblem",
    tag = "contest-problems",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = AssignProblemRequest,
    responses(
        (status = 201, description = "Problem assigned to contest", body = ContestProblemResponse),
        (status = 400, description = "Invalid alias, order, color, or problem ID", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest or problem was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Problem is already assigned or contest configuration is frozen", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn assign(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<AssignProblemRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ContestProblemResponse>), AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid problem assignment"))?;
    let assignment = state
        .contest_problems()
        .assign(contest_id, request.validate()?, context.user(), peer.ip())
        .await?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

#[utoipa::path(
    patch,
    path = "/api/contests/{contest_id}/problems/{problem_id}",
    operation_id = "updateContestProblem",
    tag = "contest-problems",
    params(("contest_id" = i64, Path), ("problem_id" = i64, Path)),
    request_body = UpdateContestProblemRequest,
    responses(
        (status = 200, description = "Contest problem assignment updated", body = ContestProblemResponse),
        (status = 400, description = "Invalid assignment update", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest problem assignment was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest configuration is frozen or assignment conflicts", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest_id, problem_id)): Path<(i64, i64)>,
    payload: Result<Json<UpdateContestProblemRequest>, JsonRejection>,
) -> Result<Json<ContestProblemResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid assignment update"))?;
    Ok(Json(
        state
            .contest_problems()
            .update(contest_id, problem_id, request.validate()?, context.user(), peer.ip())
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/contests/{contest_id}/problems/{problem_id}",
    operation_id = "removeContestProblem",
    tag = "contest-problems",
    params(("contest_id" = i64, Path), ("problem_id" = i64, Path)),
    responses(
        (status = 204, description = "Problem removed from contest"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest problem assignment was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest configuration is frozen", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn remove(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest_id, problem_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    state.contest_problems().remove(contest_id, problem_id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/contests/{contest_id}/problems/reorder",
    operation_id = "reorderContestProblems",
    tag = "contest-problems",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = Vec<ReorderEntry>,
    responses(
        (status = 200, description = "Reordered contest problems", body = [ContestProblemResponse]),
        (status = 400, description = "Invalid or duplicate reorder entries", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Contest configuration is frozen", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn reorder(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<Vec<ReorderEntry>>, JsonRejection>,
) -> Result<Json<Vec<ContestProblemResponse>>, AppError> {
    let Json(entries) =
        payload.map_err(|_| AppError::validation("request", "must be a valid reorder list"))?;
    Ok(Json(
        state
            .contest_problems()
            .reorder(contest_id, validate_reorder(entries)?, context.user(), peer.ip())
            .await?,
    ))
}
