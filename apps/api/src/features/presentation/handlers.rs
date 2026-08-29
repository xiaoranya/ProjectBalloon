use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};

use crate::{error::AppError, features::auth::AuthContext, state::AppState};

use super::model::{
    CommandRequest, CommandResponse, ConfigRequest, ConfigResponse, HeartbeatRequest,
    HeartbeatResponse, InstanceResponse, ModeQuery, PresentationTemplateRequest,
    PresentationTemplateResponse, RegisterRequest, RegistrationResponse,
};
use super::service::require_presentation_operator;

#[utoipa::path(get, path = "/api/presentation-configs/{contest_id}", operation_id = "getPresentationConfig", tag = "presentation", params(("contest_id" = i64, Path), ("mode" = String, Query)), responses((status = 200, body = ConfigResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_config(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
    Query(query): Query<ModeQuery>,
) -> Result<Json<ConfigResponse>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.presentation().config(contest, &query.mode, context.user()).await?))
}
async fn update_mode(
    context: AuthContext,
    state: AppState,
    peer: SocketAddr,
    contest: i64,
    mode: &'static str,
    payload: Result<Json<ConfigRequest>, JsonRejection>,
) -> Result<Json<ConfigResponse>, AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid presentation config"))?;
    Ok(Json(
        state
            .presentation()
            .update_config(contest, mode, request, context.user(), peer.ip())
            .await?,
    ))
}
#[utoipa::path(put, path = "/api/presentation-configs/{contest_id}/screen", operation_id = "updateScreenConfig", tag = "presentation", params(("contest_id" = i64, Path)), request_body = ConfigRequest, responses((status = 200, body = ConfigResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_screen(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<ConfigRequest>, JsonRejection>,
) -> Result<Json<ConfigResponse>, AppError> {
    update_mode(context, state, peer, contest, "SCREEN", payload).await
}
#[utoipa::path(put, path = "/api/presentation-configs/{contest_id}/live", operation_id = "updateLiveConfig", tag = "presentation", params(("contest_id" = i64, Path)), request_body = ConfigRequest, responses((status = 200, body = ConfigResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_live(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest): Path<i64>,
    payload: Result<Json<ConfigRequest>, JsonRejection>,
) -> Result<Json<ConfigResponse>, AppError> {
    update_mode(context, state, peer, contest, "LIVE", payload).await
}
#[utoipa::path(post, path = "/api/public/screens/register", operation_id = "registerScreen", tag = "screens", request_body = RegisterRequest, responses((status = 200, body = RegistrationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("csrf_cookie" = [], "csrf_header" = [])))]
pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<RegisterRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RegistrationResponse>), AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen registration"))?;
    Ok((StatusCode::CREATED, Json(state.presentation().register(request, peer.ip()).await?)))
}
#[utoipa::path(post, path = "/api/public/screens/{instance_id}/heartbeat", operation_id = "screenHeartbeat", tag = "screens", params(("instance_id" = i64, Path)), request_body = HeartbeatRequest, responses((status = 200, body = HeartbeatResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("csrf_cookie" = [], "csrf_header" = [])))]
pub async fn heartbeat(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(instance): Path<i64>,
    payload: Result<Json<HeartbeatRequest>, JsonRejection>,
) -> Result<Json<HeartbeatResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen heartbeat"))?;
    Ok(Json(state.presentation().heartbeat(instance, request, peer.ip()).await?))
}
#[utoipa::path(get, path = "/api/screen-instances/{contest_id}", operation_id = "listScreenInstances", tag = "screens", params(("contest_id" = i64, Path)), responses((status = 200, body = [InstanceResponse]), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn list_instances(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest): Path<i64>,
) -> Result<Json<Vec<InstanceResponse>>, AppError> {
    context.require_password_ready()?;
    Ok(Json(state.presentation().instances(contest, context.user()).await?))
}
#[utoipa::path(post, path = "/api/screen-instances/{contest_id}/{instance_id}/commands", operation_id = "commandScreen", tag = "screens", params(("contest_id" = i64, Path), ("instance_id" = i64, Path)), request_body = CommandRequest, responses((status = 201, body = CommandResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn command(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest, instance)): Path<(i64, i64)>,
    payload: Result<Json<CommandRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResponse>), AppError> {
    context.require_password_ready()?;
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "invalid screen command"))?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .presentation()
                .command(contest, instance, request, context.user(), peer.ip())
                .await?,
        ),
    ))
}
#[utoipa::path(delete, path = "/api/screen-instances/{contest_id}/{instance_id}", operation_id = "revokeScreen", tag = "screens", params(("contest_id" = i64, Path), ("instance_id" = i64, Path)), responses((status = 204), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn revoke(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest, instance)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    context.require_password_ready()?;
    state.presentation().revoke(contest, instance, context.user(), peer.ip()).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn validate_template_request(request: &PresentationTemplateRequest) -> Result<(), AppError> {
    if request.name.trim().is_empty() || request.name.len() > 120 || request.description.len() > 500
    {
        return Err(AppError::validation("name", "name and description are out of bounds"));
    }
    for (field, color) in [
        ("backgroundColor", &request.background_color),
        ("foregroundColor", &request.foreground_color),
        ("accentColor", &request.accent_color),
    ] {
        if color.len() != 7
            || !color.starts_with('#')
            || !color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(AppError::validation(field, "must be a six-digit hexadecimal color"));
        }
    }
    if request.font_family.trim().is_empty()
        || request.font_family.len() > 120
        || !matches!(request.density.as_str(), "COMPACT" | "COMFORTABLE" | "SPACIOUS")
    {
        return Err(AppError::validation("template", "font or density is invalid"));
    }
    if request.show_logo && request.logo_object_key.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::validation("logoObjectKey", "is required when showLogo is enabled"));
    }
    Ok(())
}

#[utoipa::path(get, path = "/api/presentation-templates", operation_id = "listPresentationTemplates", tag = "live", responses((status = 200, body = [PresentationTemplateResponse])), security(("session_cookie" = [])))]
pub async fn list_templates(
    context: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Vec<PresentationTemplateResponse>>, AppError> {
    context.require_password_ready()?;
    require_presentation_operator(context.user())?;
    Ok(Json(sqlx::query_as::<_, PresentationTemplateResponse>("SELECT id,name,description,background_color,foreground_color,accent_color,font_family,density,show_clock,show_logo,logo_object_key,updated_at FROM presentation_templates ORDER BY updated_at DESC,id DESC").fetch_all(state.database()).await.map_err(|e| AppError::internal("list presentation templates",e))?))
}

#[utoipa::path(post, path = "/api/presentation-templates", operation_id = "createPresentationTemplate", tag = "live", request_body = PresentationTemplateRequest, responses((status = 201, body = PresentationTemplateResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn create_template(
    context: AuthContext,
    State(state): State<AppState>,
    payload: Result<Json<PresentationTemplateRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PresentationTemplateResponse>), AppError> {
    context.require_password_ready()?;
    require_presentation_operator(context.user())?;
    let Json(request) = payload.map_err(|_| AppError::validation("request", "invalid template"))?;
    validate_template_request(&request)?;
    let row = sqlx::query_as::<_, PresentationTemplateResponse>(
        r#"
        INSERT INTO presentation_templates
            (name,description,background_color,foreground_color,accent_color,font_family,
             density,show_clock,show_logo,logo_object_key,created_by_user_id)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        RETURNING id,name,description,background_color,foreground_color,accent_color,
            font_family,density,show_clock,show_logo,logo_object_key,updated_at
        "#,
    )
    .bind(request.name.trim())
    .bind(request.description.trim())
    .bind(request.background_color)
    .bind(request.foreground_color)
    .bind(request.accent_color)
    .bind(request.font_family.trim())
    .bind(request.density)
    .bind(request.show_clock)
    .bind(request.show_logo)
    .bind(request.logo_object_key)
    .bind(context.user().id)
    .fetch_one(state.database())
    .await
    .map_err(|e| AppError::internal("create presentation template", e))?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(put, path = "/api/presentation-templates/{template_id}", operation_id = "updatePresentationTemplate", tag = "live", params(("template_id" = i64, Path)), request_body = PresentationTemplateRequest, responses((status = 200, body = PresentationTemplateResponse)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_template(
    context: AuthContext,
    State(state): State<AppState>,
    Path(template_id): Path<i64>,
    payload: Result<Json<PresentationTemplateRequest>, JsonRejection>,
) -> Result<Json<PresentationTemplateResponse>, AppError> {
    context.require_password_ready()?;
    require_presentation_operator(context.user())?;
    let Json(request) = payload.map_err(|_| AppError::validation("request", "invalid template"))?;
    validate_template_request(&request)?;
    let row = sqlx::query_as::<_, PresentationTemplateResponse>(
        r#"
        UPDATE presentation_templates
        SET name=$2,description=$3,background_color=$4,foreground_color=$5,accent_color=$6,
            font_family=$7,density=$8,show_clock=$9,show_logo=$10,logo_object_key=$11,
            updated_at=now()
        WHERE id=$1
        RETURNING id,name,description,background_color,foreground_color,accent_color,
            font_family,density,show_clock,show_logo,logo_object_key,updated_at
        "#,
    )
    .bind(template_id)
    .bind(request.name.trim())
    .bind(request.description.trim())
    .bind(request.background_color)
    .bind(request.foreground_color)
    .bind(request.accent_color)
    .bind(request.font_family.trim())
    .bind(request.density)
    .bind(request.show_clock)
    .bind(request.show_logo)
    .bind(request.logo_object_key)
    .fetch_optional(state.database())
    .await
    .map_err(|e| AppError::internal("update presentation template", e))?
    .ok_or_else(|| AppError::not_found("PRESENTATION_TEMPLATE_NOT_FOUND", "Template not found"))?;
    Ok(Json(row))
}
