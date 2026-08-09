use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{
    error::AppError,
    features::auth::{ContestManagerContext, SuperAdminContext},
    state::AppState,
};

use super::model::{
    BindWorkstationRequest, CreateWorkstationRequest, DeploymentInfoResponse,
    UpdateWorkstationRequest, WorkstationBindingResponse, WorkstationResponse,
};

pub async fn deployment(
    State(state): State<AppState>,
) -> Result<Json<DeploymentInfoResponse>, AppError> {
    Ok(Json(state.competition().deployment_info(state.deployment_mode()).await?))
}

pub async fn list_workstations(
    _context: ContestManagerContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkstationResponse>>, AppError> {
    require_competition(&state)?;
    Ok(Json(state.competition().list_workstations().await?))
}

pub async fn create_workstation(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    payload: Result<Json<CreateWorkstationRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<WorkstationResponse>), AppError> {
    require_competition(&state)?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid workstation object"))?;
    Ok((StatusCode::CREATED, Json(state.competition().create_workstation(request).await?)))
}

pub async fn update_workstation(
    _context: SuperAdminContext,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    payload: Result<Json<UpdateWorkstationRequest>, JsonRejection>,
) -> Result<Json<WorkstationResponse>, AppError> {
    require_competition(&state)?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid workstation object"))?;
    Ok(Json(state.competition().update_workstation(id, request).await?))
}

pub async fn list_bindings(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<Vec<WorkstationBindingResponse>>, AppError> {
    require_competition(&state)?;
    Ok(Json(state.competition().list_bindings(contest_id, context.user()).await?))
}

pub async fn bind(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<BindWorkstationRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<WorkstationBindingResponse>), AppError> {
    require_competition(&state)?;
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid workstation binding"))?;
    Ok((
        StatusCode::CREATED,
        Json(state.competition().bind(contest_id, request, context.user()).await?),
    ))
}

pub async fn rotate(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, binding_id)): Path<(i64, i64)>,
) -> Result<Json<WorkstationBindingResponse>, AppError> {
    require_competition(&state)?;
    Ok(Json(state.competition().rotate_pairing_code(contest_id, binding_id, context.user()).await?))
}

pub async fn revoke(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, binding_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    require_competition(&state)?;
    state.competition().revoke(contest_id, binding_id, context.user()).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_competition(state: &AppState) -> Result<(), AppError> {
    if state.deployment_mode().is_competition() {
        Ok(())
    } else {
        Err(AppError::not_found("COMPETITION_MODE_DISABLED", "Competition management is disabled"))
    }
}
