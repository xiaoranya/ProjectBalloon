use std::net::{IpAddr, SocketAddr};

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    error::AppError,
    features::auth::{AuthContext, model::AuthUser},
    state::AppState,
};

mod orchestration;
pub use orchestration::*;
mod live;
pub use live::*;

const SCREEN_VIEWS: &[&str] = &[
    "SCOREBOARD",
    "FIRST_BLOOD",
    "BALLOONS",
    "FREEZE_COUNTDOWN",
    "STATISTICS",
    "RESOLVER",
    "AWARDS",
];

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigRequest {
    enabled: bool,
    title: Option<String>,
    subtitle: Option<String>,
    accent_color: String,
    row_limit: i32,
    show_announcements: bool,
    announcement_interval_seconds: i32,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    custom_template_id: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    contest_id: i64,
    mode: String,
    enabled: bool,
    title: Option<String>,
    subtitle: Option<String>,
    accent_color: String,
    row_limit: i32,
    show_announcements: bool,
    announcement_interval_seconds: i32,
    template: String,
    custom_template_id: Option<i64>,
    custom_template_name: Option<String>,
    custom_background_color: Option<String>,
    custom_foreground_color: Option<String>,
    custom_accent_color: Option<String>,
    custom_font_family: Option<String>,
    custom_density: Option<String>,
    custom_show_clock: Option<bool>,
    custom_show_logo: Option<bool>,
    custom_logo_object_key: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterRequest {
    contest_id: i64,
    name: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationResponse {
    instance_id: i64,
    contest_id: i64,
    name: String,
    client_token: String,
    current_view: String,
    #[serde(with = "time::serde::rfc3339")]
    registered_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeartbeatRequest {
    client_token: String,
    current_view: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    instance_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    server_time: OffsetDateTime,
    command_id: Option<i64>,
    target_view: Option<String>,
    group_playback: Option<GroupPlaybackResponse>,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct InstanceResponse {
    id: i64,
    contest_id: i64,
    name: String,
    current_view: String,
    online: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    last_seen_at: Option<OffsetDateTime>,
    last_ip: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    revoked_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRequest {
    target_view: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    id: i64,
    screen_instance_id: i64,
    target_view: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModeQuery {
    mode: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTemplateResponse {
    id: i64,
    name: String,
    description: String,
    background_color: String,
    foreground_color: String,
    accent_color: String,
    font_family: String,
    density: String,
    show_clock: bool,
    show_logo: bool,
    logo_object_key: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationTemplateRequest {
    name: String,
    #[serde(default)]
    description: String,
    background_color: String,
    foreground_color: String,
    accent_color: String,
    #[serde(default = "default_font")]
    font_family: String,
    #[serde(default = "default_density")]
    density: String,
    #[serde(default = "default_true")]
    show_clock: bool,
    #[serde(default)]
    show_logo: bool,
    logo_object_key: Option<String>,
}

fn default_font() -> String {
    "Inter".to_owned()
}
fn default_density() -> String {
    "COMFORTABLE".to_owned()
}
const fn default_true() -> bool {
    true
}

pub struct PresentationService {
    database: PgPool,
}

impl PresentationService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn config(
        &self,
        contest: i64,
        mode: &str,
        actor: &AuthUser,
    ) -> Result<ConfigResponse, AppError> {
        require_presentation_operator(actor)?;
        let mode = validate_mode(mode)?;
        require_contest(&self.database, contest).await?;
        Ok(sqlx::query_as::<_, ConfigResponse>("SELECT c.contest_id,c.mode,c.enabled,c.title,c.subtitle,c.accent_color,c.row_limit,c.show_announcements,c.announcement_interval_seconds,c.template,c.custom_template_id,t.name AS custom_template_name,t.background_color AS custom_background_color,t.foreground_color AS custom_foreground_color,t.accent_color AS custom_accent_color,t.font_family AS custom_font_family,t.density AS custom_density,t.show_clock AS custom_show_clock,t.show_logo AS custom_show_logo,t.logo_object_key AS custom_logo_object_key,c.updated_at FROM presentation_configs c LEFT JOIN presentation_templates t ON t.id=c.custom_template_id WHERE c.contest_id=$1 AND c.mode=$2")
            .bind(contest).bind(mode).fetch_optional(&self.database).await.map_err(|error| AppError::internal("load presentation config", error))?
            .unwrap_or(ConfigResponse { contest_id: contest, mode: mode.to_owned(), enabled: false, title: None, subtitle: None, accent_color: "#22c55e".into(), row_limit: 12, show_announcements: true, announcement_interval_seconds: 10, template: "DEFAULT".into(), custom_template_id: None, custom_template_name: None, custom_background_color: None, custom_foreground_color: None, custom_accent_color: None, custom_font_family: None, custom_density: None, custom_show_clock: None, custom_show_logo: None, custom_logo_object_key: None, updated_at: None }))
    }

    async fn update_config(
        &self,
        contest: i64,
        mode: &str,
        request: ConfigRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<ConfigResponse, AppError> {
        let mode = validate_mode(mode)?;
        require_mode_operator(actor, mode)?;
        validate_config(&request)?;
        require_contest(&self.database, contest).await?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin presentation config", error))?;
        let template = validate_template(request.template.as_deref().unwrap_or("DEFAULT"))?;
        if template == "CUSTOM" {
            let id = request.custom_template_id.ok_or_else(|| {
                AppError::validation("customTemplateId", "is required for a custom template")
            })?;
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM presentation_templates WHERE id=$1)",
            )
            .bind(id)
            .fetch_one(&self.database)
            .await
            .map_err(|e| AppError::internal("check custom presentation template", e))?;
            if !exists {
                return Err(AppError::not_found(
                    "PRESENTATION_TEMPLATE_NOT_FOUND",
                    "Custom template not found",
                ));
            }
        } else if request.custom_template_id.is_some() {
            return Err(AppError::validation(
                "customTemplateId",
                "is only valid with the CUSTOM template",
            ));
        }
        sqlx::query("INSERT INTO presentation_configs(contest_id,mode,enabled,title,subtitle,accent_color,row_limit,show_announcements,announcement_interval_seconds,template,custom_template_id,updated_by_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(contest_id,mode) DO UPDATE SET enabled=excluded.enabled,title=excluded.title,subtitle=excluded.subtitle,accent_color=excluded.accent_color,row_limit=excluded.row_limit,show_announcements=excluded.show_announcements,announcement_interval_seconds=excluded.announcement_interval_seconds,template=excluded.template,custom_template_id=excluded.custom_template_id,updated_by_user_id=excluded.updated_by_user_id,updated_at=now()")
            .bind(contest).bind(mode).bind(request.enabled).bind(request.title.as_deref()).bind(request.subtitle.as_deref()).bind(&request.accent_color).bind(request.row_limit).bind(request.show_announcements).bind(request.announcement_interval_seconds).bind(template).bind(request.custom_template_id).bind(actor.id)
            .execute(&mut *tx).await.map_err(|error| AppError::internal("save presentation config", error))?;
        audit(&mut tx, actor.id, "PRESENTATION_CONFIG_UPDATED", "CONTEST", contest, ip).await?;
        sqlx::query("INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,payload_json) VALUES($1,$2,'PRESENTATION_UPDATED','PUBLIC',$3)")
            .bind(uuid::Uuid::new_v4()).bind(contest).bind(serde_json::json!({"mode":mode})).execute(&mut *tx).await.map_err(|error| AppError::internal("publish presentation config", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit presentation config", error))?;
        self.config(contest, mode, actor).await
    }

    async fn register(
        &self,
        mut request: RegisterRequest,
        ip: IpAddr,
    ) -> Result<RegistrationResponse, AppError> {
        request.name = request.name.trim().to_owned();
        if request.contest_id <= 0 || request.name.is_empty() || request.name.chars().count() > 120
        {
            return Err(AppError::validation(
                "screen",
                "contestId and a name up to 120 characters are required",
            ));
        }
        require_contest(&self.database, request.contest_id).await?;
        let enabled = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM presentation_configs WHERE contest_id=$1 AND mode='SCREEN' AND enabled)")
            .bind(request.contest_id).fetch_one(&self.database).await.map_err(|error| AppError::internal("check screen publication", error))?;
        if !enabled {
            return Err(AppError::conflict(
                "SCREEN_PRESENTATION_NOT_PUBLISHED",
                "Screen presentation is not published",
            ));
        }
        let recent = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM screen_instances WHERE last_ip = $1 AND created_at > now() - interval '10 minutes'",
        )
        .bind(ip.to_string())
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check screen registration rate", error))?;
        if recent >= 20 {
            return Err(AppError::too_many_requests(
                "SCREEN_REGISTRATION_RATE_LIMITED",
                "Too many screen registrations; try again later",
            ));
        }
        let mut raw = [0_u8; 32];
        getrandom::fill(&mut raw)
            .map_err(|error| AppError::internal("generate screen token", error))?;
        let token = URL_SAFE_NO_PAD.encode(raw);
        let hash = token_hash(&token);
        let row = sqlx::query_as::<_, (i64, OffsetDateTime)>("INSERT INTO screen_instances(contest_id,name,client_token_hash,current_view,last_seen_at,last_ip) VALUES($1,$2,$3,'SCOREBOARD',now(),$4) RETURNING id,created_at")
            .bind(request.contest_id).bind(&request.name).bind(hash).bind(ip.to_string()).fetch_one(&self.database).await.map_err(|error| AppError::internal("register screen instance", error))?;
        Ok(RegistrationResponse {
            instance_id: row.0,
            contest_id: request.contest_id,
            name: request.name,
            client_token: token,
            current_view: "SCOREBOARD".into(),
            registered_at: row.1,
        })
    }

    async fn heartbeat(
        &self,
        instance: i64,
        mut request: HeartbeatRequest,
        ip: IpAddr,
    ) -> Result<HeartbeatResponse, AppError> {
        request.current_view = validate_view(&request.current_view)?.to_owned();
        if request.client_token.is_empty() || request.client_token.len() > 256 {
            return Err(AppError::unauthorized("SCREEN_TOKEN_INVALID", "Screen token is invalid"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin screen heartbeat", error))?;
        let updated = sqlx::query_scalar::<_, i64>("UPDATE screen_instances SET current_view=$3,last_seen_at=now(),last_ip=$4,updated_at=now() WHERE id=$1 AND client_token_hash=$2 AND revoked_at IS NULL RETURNING id")
            .bind(instance).bind(token_hash(&request.client_token)).bind(&request.current_view).bind(ip.to_string()).fetch_optional(&mut *tx).await.map_err(|error| AppError::internal("update screen heartbeat", error))?;
        if updated.is_none() {
            return Err(AppError::unauthorized("SCREEN_TOKEN_INVALID", "Screen token is invalid"));
        }
        let command = sqlx::query_as::<_, (i64, String)>("SELECT id,target_view FROM screen_commands WHERE screen_instance_id=$1 AND acknowledged_at IS NULL ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE")
            .bind(instance).fetch_optional(&mut *tx).await.map_err(|error| AppError::internal("load screen command", error))?;
        sqlx::query("UPDATE screen_commands SET acknowledged_at=now() WHERE screen_instance_id=$1 AND acknowledged_at IS NULL")
            .bind(instance).execute(&mut *tx).await.map_err(|error| AppError::internal("acknowledge screen commands", error))?;
        let group_playback = orchestration::playback_for_instance(&mut tx, instance).await?;
        tx.commit().await.map_err(|error| AppError::internal("commit screen heartbeat", error))?;
        Ok(HeartbeatResponse {
            instance_id: instance,
            server_time: OffsetDateTime::now_utc(),
            command_id: command.as_ref().map(|row| row.0),
            target_view: command.map(|row| row.1),
            group_playback,
        })
    }

    async fn instances(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<InstanceResponse>, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        sqlx::query_as("SELECT id,contest_id,name,current_view,(revoked_at IS NULL AND last_seen_at >= now()-interval '45 seconds') AS online,last_seen_at,last_ip,revoked_at,created_at FROM screen_instances WHERE contest_id=$1 ORDER BY created_at,id")
            .bind(contest).fetch_all(&self.database).await.map_err(|error| AppError::internal("list screen instances", error))
    }

    async fn command(
        &self,
        contest: i64,
        instance: i64,
        request: CommandRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CommandResponse, AppError> {
        require_screen_operator(actor)?;
        let target = validate_view(&request.target_view)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin screen command", error))?;
        let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM screen_instances WHERE id=$1 AND contest_id=$2 AND revoked_at IS NULL)")
            .bind(instance).bind(contest).fetch_one(&mut *tx).await.map_err(|error| AppError::internal("check screen instance", error))?;
        if !exists {
            return Err(AppError::not_found(
                "SCREEN_INSTANCE_NOT_FOUND",
                "Screen instance was not found",
            ));
        }
        let command = sqlx::query_as::<_, CommandResponse>("INSERT INTO screen_commands(screen_instance_id,target_view,created_by_user_id) VALUES($1,$2,$3) RETURNING id,screen_instance_id,target_view,created_at")
            .bind(instance).bind(target).bind(actor.id).fetch_one(&mut *tx).await.map_err(|error| AppError::internal("create screen command", error))?;
        audit(&mut tx, actor.id, "SCREEN_COMMAND_SENT", "SCREEN_INSTANCE", instance, ip).await?;
        tx.commit().await.map_err(|error| AppError::internal("commit screen command", error))?;
        Ok(command)
    }

    async fn revoke(
        &self,
        contest: i64,
        instance: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
        require_screen_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin screen revoke", error))?;
        let changed = sqlx::query("UPDATE screen_instances SET revoked_at=coalesce(revoked_at,now()),updated_at=now() WHERE id=$1 AND contest_id=$2")
            .bind(instance).bind(contest).execute(&mut *tx).await.map_err(|error| AppError::internal("revoke screen instance", error))?.rows_affected();
        if changed != 1 {
            return Err(AppError::not_found(
                "SCREEN_INSTANCE_NOT_FOUND",
                "Screen instance was not found",
            ));
        }
        sqlx::query("DELETE FROM screen_group_members WHERE screen_instance_id=$1")
            .bind(instance)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("remove revoked screen from groups", error))?;
        audit(&mut tx, actor.id, "SCREEN_INSTANCE_REVOKED", "SCREEN_INSTANCE", instance, ip)
            .await?;
        tx.commit().await.map_err(|error| AppError::internal("commit screen revoke", error))?;
        Ok(())
    }
}

pub(super) async fn require_contest(database: &PgPool, contest: i64) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
    )
    .bind(contest)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check presentation contest", error))?;
    if exists {
        Ok(())
    } else {
        Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))
    }
}

fn validate_mode(mode: &str) -> Result<&'static str, AppError> {
    match mode.trim().to_ascii_uppercase().as_str() {
        "SCREEN" => Ok("SCREEN"),
        "LIVE" => Ok("LIVE"),
        _ => Err(AppError::validation("mode", "must be SCREEN or LIVE")),
    }
}

pub(super) fn validate_view(view: &str) -> Result<&'static str, AppError> {
    let normalized = view.trim().to_ascii_uppercase();
    SCREEN_VIEWS
        .iter()
        .copied()
        .find(|value| *value == normalized)
        .ok_or_else(|| AppError::validation("targetView", "is not a supported screen view"))
}

fn validate_config(request: &ConfigRequest) -> Result<(), AppError> {
    let color = request.accent_color.as_bytes();
    let valid_color =
        color.len() == 7 && color[0] == b'#' && color[1..].iter().all(u8::is_ascii_hexdigit);
    if !valid_color {
        return Err(AppError::validation("accentColor", "must be a six-digit hex color"));
    }
    if !(5..=30).contains(&request.row_limit) {
        return Err(AppError::validation("rowLimit", "must be between 5 and 30"));
    }
    if !(5..=60).contains(&request.announcement_interval_seconds) {
        return Err(AppError::validation(
            "announcementIntervalSeconds",
            "must be between 5 and 60",
        ));
    }
    if request.title.as_ref().is_some_and(|value| value.chars().count() > 160)
        || request.subtitle.as_ref().is_some_and(|value| value.chars().count() > 240)
    {
        return Err(AppError::validation("title", "title or subtitle is too long"));
    }
    validate_template(request.template.as_deref().unwrap_or("DEFAULT"))?;
    Ok(())
}

fn validate_template(template: &str) -> Result<&str, AppError> {
    match template.trim() {
        "DEFAULT" | "CINEMATIC" | "MINIMAL" | "SPLIT" | "CUSTOM" => Ok(template.trim()),
        _ => Err(AppError::validation(
            "template",
            "must be DEFAULT, CINEMATIC, MINIMAL, SPLIT, or CUSTOM",
        )),
    }
}

fn require_presentation_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN")
        || actor.has_role("SCREEN_OPERATOR")
        || actor.has_role("LIVE_OPERATOR")
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "PRESENTATION_OPERATOR_REQUIRED",
            "Presentation operator role is required",
        ))
    }
}
pub(super) fn require_screen_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") || actor.has_role("SCREEN_OPERATOR") {
        Ok(())
    } else {
        Err(AppError::forbidden("SCREEN_OPERATOR_REQUIRED", "Screen operator role is required"))
    }
}
fn require_mode_operator(actor: &AuthUser, mode: &str) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN")
        || (mode == "SCREEN" && actor.has_role("SCREEN_OPERATOR"))
        || (mode == "LIVE" && actor.has_role("LIVE_OPERATOR"))
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "PRESENTATION_OPERATOR_REQUIRED",
            "Presentation operator role is required",
        ))
    }
}
fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub(super) async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: i64,
    action: &str,
    target_type: &str,
    target: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES($1,$2,$3,$4,$5,'success')")
        .bind(actor).bind(action).bind(target_type).bind(target.to_string()).bind(ip.to_string()).execute(&mut **tx).await.map(|_| ()).map_err(|error| AppError::internal("record presentation audit", error))
}

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
    let row=sqlx::query_as::<_,PresentationTemplateResponse>("INSERT INTO presentation_templates(name,description,background_color,foreground_color,accent_color,font_family,density,show_clock,show_logo,logo_object_key,created_by_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id,name,description,background_color,foreground_color,accent_color,font_family,density,show_clock,show_logo,logo_object_key,updated_at").bind(request.name.trim()).bind(request.description.trim()).bind(request.background_color).bind(request.foreground_color).bind(request.accent_color).bind(request.font_family.trim()).bind(request.density).bind(request.show_clock).bind(request.show_logo).bind(request.logo_object_key).bind(context.user().id).fetch_one(state.database()).await.map_err(|e|AppError::internal("create presentation template",e))?;
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
    let row=sqlx::query_as::<_,PresentationTemplateResponse>("UPDATE presentation_templates SET name=$2,description=$3,background_color=$4,foreground_color=$5,accent_color=$6,font_family=$7,density=$8,show_clock=$9,show_logo=$10,logo_object_key=$11,updated_at=now() WHERE id=$1 RETURNING id,name,description,background_color,foreground_color,accent_color,font_family,density,show_clock,show_logo,logo_object_key,updated_at").bind(template_id).bind(request.name.trim()).bind(request.description.trim()).bind(request.background_color).bind(request.foreground_color).bind(request.accent_color).bind(request.font_family.trim()).bind(request.density).bind(request.show_clock).bind(request.show_logo).bind(request.logo_object_key).fetch_optional(state.database()).await.map_err(|e|AppError::internal("update presentation template",e))?.ok_or_else(||AppError::not_found("PRESENTATION_TEMPLATE_NOT_FOUND","Template not found"))?;
    Ok(Json(row))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use super::*;
    use crate::features::auth::model::UserType;

    #[test]
    fn presentation_domains_are_closed() {
        assert_eq!(validate_mode("screen").expect("screen"), "SCREEN");
        assert!(validate_mode("OBS").is_err());
        assert_eq!(validate_view("awards").expect("awards"), "AWARDS");
        assert!(validate_view("javascript:").is_err());
        assert_eq!(validate_template(" CINEMATIC ").expect("template"), "CINEMATIC");
        assert!(validate_template("CUSTOM_HTML").is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn screen_registration_heartbeat_commands_and_revocation_are_atomic(pool: PgPool) {
        let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('screen-op','hash','Screen Op','SCREEN_OPERATOR') RETURNING id")
            .fetch_one(&pool).await.expect("screen operator");
        let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Screen Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id")
            .fetch_one(&pool).await.expect("contest");
        let actor = AuthUser {
            id: user,
            username: "screen-op".into(),
            display_name: "Screen Op".into(),
            user_type: UserType::ScreenOperator,
            roles: vec!["SCREEN_OPERATOR".into()],
            password_reset_required: false,
        };
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let service = PresentationService::new(pool.clone());
        assert!(
            service
                .register(RegisterRequest { contest_id: contest, name: "Main Hall".into() }, ip)
                .await
                .is_err()
        );
        let config = service
            .update_config(
                contest,
                "SCREEN",
                ConfigRequest {
                    enabled: true,
                    title: Some("Finals".into()),
                    subtitle: None,
                    accent_color: "#22c55e".into(),
                    row_limit: 12,
                    show_announcements: true,
                    announcement_interval_seconds: 10,
                    template: Some("CINEMATIC".into()),
                    custom_template_id: None,
                },
                &actor,
                ip,
            )
            .await
            .expect("publish screen");
        assert!(config.enabled);
        assert_eq!(config.template, "CINEMATIC");
        let registration = service
            .register(RegisterRequest { contest_id: contest, name: " Main Hall ".into() }, ip)
            .await
            .expect("register");
        assert_eq!(registration.name, "Main Hall");
        let stored_hash = sqlx::query_scalar::<_, String>(
            "SELECT client_token_hash FROM screen_instances WHERE id=$1",
        )
        .bind(registration.instance_id)
        .fetch_one(&pool)
        .await
        .expect("token hash");
        assert_ne!(stored_hash, registration.client_token);
        assert_eq!(stored_hash, token_hash(&registration.client_token));
        assert!(service.instances(contest, &actor).await.expect("instances")[0].online);
        service
            .command(
                contest,
                registration.instance_id,
                CommandRequest { target_view: "SCOREBOARD".into() },
                &actor,
                ip,
            )
            .await
            .expect("first command");
        let latest = service
            .command(
                contest,
                registration.instance_id,
                CommandRequest { target_view: "AWARDS".into() },
                &actor,
                ip,
            )
            .await
            .expect("latest command");
        let heartbeat = service
            .heartbeat(
                registration.instance_id,
                HeartbeatRequest {
                    client_token: registration.client_token.clone(),
                    current_view: "STATISTICS".into(),
                },
                ip,
            )
            .await
            .expect("heartbeat");
        assert_eq!(heartbeat.command_id, Some(latest.id));
        assert_eq!(heartbeat.target_view.as_deref(), Some("AWARDS"));
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM screen_commands WHERE screen_instance_id=$1 AND acknowledged_at IS NOT NULL").bind(registration.instance_id).fetch_one(&pool).await.expect("acked"), 2);
        assert!(
            service
                .heartbeat(
                    registration.instance_id,
                    HeartbeatRequest {
                        client_token: "wrong".into(),
                        current_view: "SCOREBOARD".into()
                    },
                    ip
                )
                .await
                .is_err()
        );
        sqlx::query(
            "UPDATE screen_instances SET last_seen_at=now()-interval '1 minute' WHERE id=$1",
        )
        .bind(registration.instance_id)
        .execute(&pool)
        .await
        .expect("age heartbeat");
        assert!(!service.instances(contest, &actor).await.expect("offline instance")[0].online);
        let group = sqlx::query_scalar::<_, i64>("INSERT INTO screen_groups(contest_id,name,created_by_user_id) VALUES($1,'Hall',$2) RETURNING id").bind(contest).bind(user).fetch_one(&pool).await.expect("group");
        sqlx::query("INSERT INTO screen_group_members(group_id,screen_instance_id) VALUES($1,$2)")
            .bind(group)
            .bind(registration.instance_id)
            .execute(&pool)
            .await
            .expect("group member");
        service.revoke(contest, registration.instance_id, &actor, ip).await.expect("revoke");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM screen_group_members WHERE screen_instance_id=$1"
            )
            .bind(registration.instance_id)
            .fetch_one(&pool)
            .await
            .expect("members"),
            0
        );
        assert!(
            service
                .heartbeat(
                    registration.instance_id,
                    HeartbeatRequest {
                        client_token: registration.client_token,
                        current_view: "SCOREBOARD".into()
                    },
                    ip
                )
                .await
                .is_err()
        );
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE actor_user_id=$1 AND action IN ('PRESENTATION_CONFIG_UPDATED','SCREEN_COMMAND_SENT','SCREEN_INSTANCE_REVOKED')").bind(user).fetch_one(&pool).await.expect("audit"), 4);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox WHERE contest_id=$1 AND event_type='PRESENTATION_UPDATED'").bind(contest).fetch_one(&pool).await.expect("outbox"), 1);
    }
}
