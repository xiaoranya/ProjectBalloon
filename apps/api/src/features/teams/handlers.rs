use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext, OptionalAuthContext, SuperAdminContext},
    pagination::PageResponse,
    state::AppState,
};

use super::model::{
    BatchImportRequest, BatchImportResponse, ContestTeamAssignmentRequest, ContestTeamResponse,
    CreateTeamRequest, ResetTeamPasswordRequest, TeamListQuery, TeamMemberPatchRequest,
    TeamMemberRequest, TeamMemberResponse, TeamResponse, UpdateTeamRequest,
};

#[utoipa::path(
    post,
    path = "/api/teams",
    operation_id = "createTeam",
    tag = "teams",
    request_body = CreateTeamRequest,
    responses(
        (status = 201, description = "Team created", body = TeamResponse),
        (status = 400, description = "Invalid team or account fields", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Super administrator, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 409, description = "Team name, username, or team role conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn create(
    context: SuperAdminContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<CreateTeamRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TeamResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid team object"))?;
    let team = state.teams().create(request.validate()?, context.user().id, peer.ip()).await?;
    Ok((StatusCode::CREATED, Json(team)))
}

#[utoipa::path(
    get,
    path = "/api/teams",
    operation_id = "listTeams",
    tag = "teams",
    params(
        ("page" = Option<u32>, Query, description = "Zero-based page index; defaults to 0"),
        ("size" = Option<u32>, Query, description = "Page size from 1 through 500; defaults to 100"),
        ("sort" = Option<String>, Query, description = "Allowed sort: name, createdAt, or updatedAt with asc/desc"),
        ("includeDeleted" = Option<bool>, Query, description = "Include soft-deleted teams; super administrator only")
    ),
    responses(
        (status = 200, description = "Teams visible to the authenticated actor", body = PageResponse<TeamResponse>),
        (status = 400, description = "Invalid pagination or sort filter", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Team account, mandatory password reset, or deleted-team filter is not permitted", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list(
    context: AuthContext,
    State(state): State<AppState>,
    query: Result<Query<TeamListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PageResponse<TeamResponse>>, AppError> {
    context.require_password_ready()?;
    let Query(query) =
        query.map_err(|_| AppError::validation("query", "must contain valid team filters"))?;
    Ok(Json(state.teams().list(query.validate()?, context.user()).await?))
}

#[utoipa::path(
    get,
    path = "/api/teams/{team_id}",
    operation_id = "getTeam",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    responses(
        (status = 200, description = "Team visible to the authenticated actor", body = TeamResponse),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Password reset or access policy prevents this operation", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team was not found or is outside the actor's scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn get(
    context: AuthContext,
    State(state): State<AppState>,
    Path(team_id): Path<i64>,
) -> Result<Json<TeamResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.teams().get(team_id, context.user()).await?))
}

#[utoipa::path(
    patch,
    path = "/api/teams/{team_id}",
    operation_id = "updateTeam",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    request_body = UpdateTeamRequest,
    responses(
        (status = 200, description = "Team updated", body = TeamResponse),
        (status = 400, description = "Invalid team update", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Team name or optimistic version conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(team_id): Path<i64>,
    payload: Result<Json<UpdateTeamRequest>, JsonRejection>,
) -> Result<Json<TeamResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid team update"))?;
    Ok(Json(state.teams().update(team_id, request.validate()?, context.user(), peer.ip()).await?))
}

#[utoipa::path(
    delete,
    path = "/api/teams/{team_id}",
    operation_id = "deleteTeam",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    responses(
        (status = 204, description = "Team soft-deleted and its account disabled"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Team is still assigned to a contest", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn delete(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(team_id): Path<i64>,
) -> Result<StatusCode, AppError> {
    state.teams().delete(team_id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/teams/{team_id}/members",
    operation_id = "addTeamMember",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    request_body = TeamMemberRequest,
    responses(
        (status = 201, description = "Team member added", body = TeamMemberResponse),
        (status = 400, description = "Invalid member fields", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team was not found or is outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn add_member(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(team_id): Path<i64>,
    payload: Result<Json<TeamMemberRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TeamMemberResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid team member"))?;
    let member =
        state.teams().add_member(team_id, request.validate()?, context.user(), peer.ip()).await?;
    Ok((StatusCode::CREATED, Json(member)))
}

#[utoipa::path(
    get,
    path = "/api/teams/{team_id}/members",
    operation_id = "listTeamMembers",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    responses(
        (status = 200, description = "Team members", body = [TeamMemberResponse]),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access and completed password reset required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team was not found or is outside management scope", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn list_members(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(team_id): Path<i64>,
) -> Result<Json<Vec<TeamMemberResponse>>, AppError> {
    Ok(Json(state.teams().list_members(team_id, context.user()).await?))
}

#[utoipa::path(
    patch,
    path = "/api/teams/{team_id}/members/{member_id}",
    operation_id = "updateTeamMember",
    tag = "teams",
    params(
        ("team_id" = i64, Path, description = "Team identifier"),
        ("member_id" = i64, Path, description = "Team member identifier")
    ),
    request_body = TeamMemberPatchRequest,
    responses(
        (status = 200, description = "Team member updated", body = TeamMemberResponse),
        (status = 400, description = "Invalid member update", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team or member was not found", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn update_member(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((team_id, member_id)): Path<(i64, i64)>,
    payload: Result<Json<TeamMemberPatchRequest>, JsonRejection>,
) -> Result<Json<TeamMemberResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid member update"))?;
    Ok(Json(
        state
            .teams()
            .update_member(team_id, member_id, request.validate()?, context.user(), peer.ip())
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/teams/{team_id}/members/{member_id}",
    operation_id = "removeTeamMember",
    tag = "teams",
    params(
        ("team_id" = i64, Path, description = "Team identifier"),
        ("member_id" = i64, Path, description = "Team member identifier")
    ),
    responses(
        (status = 204, description = "Team member removed"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team or member was not found", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn remove_member(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((team_id, member_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    state.teams().remove_member(team_id, member_id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/contests/{contest_id}/teams",
    operation_id = "assignTeamToContest",
    tag = "teams",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    request_body = ContestTeamAssignmentRequest,
    responses(
        (status = 201, description = "Team assigned to contest", body = ContestTeamResponse),
        (status = 400, description = "Invalid team assignment", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest or team was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Roster is closed or team is already assigned", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn assign_to_contest(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<ContestTeamAssignmentRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<ContestTeamResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid team assignment"))?;
    let assignment = state
        .teams()
        .assign_to_contest(contest_id, request.validate()?, context.user(), peer.ip())
        .await?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

#[utoipa::path(
    get,
    path = "/api/contests/{contest_id}/teams",
    operation_id = "listContestTeams",
    tag = "teams",
    params(("contest_id" = i64, Path, description = "Contest identifier")),
    responses(
        (status = 200, description = "Contest roster", body = [ContestTeamResponse]),
        (status = 401, description = "Supplied session is invalid or expired", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is not visible", body = crate::error::ApiErrorBody)
    ),
    security((), ("session_cookie" = []))
)]
pub async fn list_contest_teams(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<ContestTeamResponse>>, AppError> {
    state.contests().get(contest_id, context.user()).await?;
    Ok(Json(state.teams().list_contest_teams(contest_id).await?))
}

#[utoipa::path(
    delete,
    path = "/api/contests/{contest_id}/teams/{team_id}",
    operation_id = "removeTeamFromContest",
    tag = "teams",
    params(
        ("contest_id" = i64, Path, description = "Contest identifier"),
        ("team_id" = i64, Path, description = "Team identifier")
    ),
    responses(
        (status = 204, description = "Team removed from contest roster"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest or roster assignment was not found", body = crate::error::ApiErrorBody),
        (status = 409, description = "Roster is closed", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn remove_from_contest(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest_id, team_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    state.teams().remove_from_contest(contest_id, team_id, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/teams/{team_id}/account/reset-password",
    operation_id = "resetTeamPassword",
    tag = "teams",
    params(("team_id" = i64, Path, description = "Team identifier")),
    request_body = ResetTeamPasswordRequest,
    responses(
        (status = 204, description = "Team password reset and existing sessions revoked"),
        (status = 400, description = "Invalid password", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Team or account was not found", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn reset_password(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(team_id): Path<i64>,
    payload: Result<Json<ResetTeamPasswordRequest>, JsonRejection>,
) -> Result<StatusCode, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must contain a new password"))?;
    state.teams().reset_password(team_id, request.validate()?, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/teams/batch",
    operation_id = "batchImportTeams",
    tag = "teams",
    request_body = BatchImportRequest,
    responses(
        (status = 200, description = "Idempotent team import result", body = BatchImportResponse),
        (status = 400, description = "Invalid import payload or contest requirement", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "Contest management access, completed password reset, and CSRF protection required", body = crate::error::ApiErrorBody),
        (status = 404, description = "Contest was not found or is outside management scope", body = crate::error::ApiErrorBody),
        (status = 409, description = "Idempotency, duplicate, role, or roster conflict", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn batch_import(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<BatchImportRequest>, JsonRejection>,
) -> Result<Json<BatchImportResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid team import"))?;
    Ok(Json(state.teams().batch_import(request.validate()?, context.user(), peer.ip()).await?))
}
