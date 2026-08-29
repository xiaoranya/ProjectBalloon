use std::net::IpAddr;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::{error::AppError, features::auth::model::AuthUser};

use crate::features::presentation::model::{
    CommandRequest, CommandResponse, ConfigRequest, ConfigResponse, HeartbeatRequest,
    HeartbeatResponse, InstanceResponse, RegisterRequest, RegistrationResponse,
};
use crate::features::presentation::orchestration::playback_for_instance;

const SCREEN_VIEWS: &[&str] = &[
    "SCOREBOARD",
    "FIRST_BLOOD",
    "BALLOONS",
    "FREEZE_COUNTDOWN",
    "STATISTICS",
    "RESOLVER",
    "AWARDS",
];

pub struct PresentationService {
    database: PgPool,
}

impl PresentationService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(super) async fn config(
        &self,
        contest: i64,
        mode: &str,
        actor: &AuthUser,
    ) -> Result<ConfigResponse, AppError> {
        require_presentation_operator(actor)?;
        let mode = validate_mode(mode)?;
        require_contest(&self.database, contest).await?;
        Ok(sqlx::query_as::<_, ConfigResponse>(
            r#"
            SELECT c.contest_id,c.mode,c.enabled,c.title,c.subtitle,c.accent_color,c.row_limit,
                   c.show_announcements,c.announcement_interval_seconds,c.template,
                   c.custom_template_id,t.name AS custom_template_name,
                   t.background_color AS custom_background_color,
                   t.foreground_color AS custom_foreground_color,
                   t.accent_color AS custom_accent_color,
                   t.font_family AS custom_font_family,t.density AS custom_density,
                   t.show_clock AS custom_show_clock,t.show_logo AS custom_show_logo,
                   t.logo_object_key AS custom_logo_object_key,c.updated_at
            FROM presentation_configs c
            LEFT JOIN presentation_templates t
                ON t.id=c.custom_template_id
            WHERE c.contest_id=$1 AND c.mode=$2
            "#,
        )
        .bind(contest)
        .bind(mode)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load presentation config", error))?
        .unwrap_or(ConfigResponse {
            contest_id: contest,
            mode: mode.to_owned(),
            enabled: false,
            title: None,
            subtitle: None,
            accent_color: "#22c55e".into(),
            row_limit: 12,
            show_announcements: true,
            announcement_interval_seconds: 10,
            template: "DEFAULT".into(),
            custom_template_id: None,
            custom_template_name: None,
            custom_background_color: None,
            custom_foreground_color: None,
            custom_accent_color: None,
            custom_font_family: None,
            custom_density: None,
            custom_show_clock: None,
            custom_show_logo: None,
            custom_logo_object_key: None,
            updated_at: None,
        }))
    }

    pub(super) async fn update_config(
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
            .fetch_one(&mut *tx)
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
        sqlx::query(
            r#"
            INSERT INTO presentation_configs
                (contest_id,mode,enabled,title,subtitle,accent_color,row_limit,
                 show_announcements,announcement_interval_seconds,template,
                 custom_template_id,updated_by_user_id)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT(contest_id,mode) DO UPDATE
                SET enabled=excluded.enabled,
                    title=excluded.title,
                    subtitle=excluded.subtitle,
                    accent_color=excluded.accent_color,
                    row_limit=excluded.row_limit,
                    show_announcements=excluded.show_announcements,
                    announcement_interval_seconds=excluded.announcement_interval_seconds,
                    template=excluded.template,
                    custom_template_id=excluded.custom_template_id,
                    updated_by_user_id=excluded.updated_by_user_id,
                    updated_at=now()
            "#,
        )
        .bind(contest)
        .bind(mode)
        .bind(request.enabled)
        .bind(request.title.as_deref())
        .bind(request.subtitle.as_deref())
        .bind(&request.accent_color)
        .bind(request.row_limit)
        .bind(request.show_announcements)
        .bind(request.announcement_interval_seconds)
        .bind(template)
        .bind(request.custom_template_id)
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("save presentation config", error))?;
        audit(&mut tx, actor.id, "PRESENTATION_CONFIG_UPDATED", "CONTEST", contest, ip).await?;
        sqlx::query("INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,payload_json) VALUES($1,$2,'PRESENTATION_UPDATED','PUBLIC',$3)")
            .bind(uuid::Uuid::new_v4()).bind(contest).bind(serde_json::json!({"mode":mode})).execute(&mut *tx).await.map_err(|error| AppError::internal("publish presentation config", error))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit presentation config", error))?;
        self.config(contest, mode, actor).await
    }

    pub(super) async fn register(
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

    pub(super) async fn heartbeat(
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
        let updated = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE screen_instances instance
            SET current_view=$3,last_seen_at=now(),last_ip=$4,updated_at=now()
            WHERE instance.id=$1 AND instance.client_token_hash=$2
                AND instance.revoked_at IS NULL
                AND EXISTS (
                    SELECT 1 FROM contests contest
                    WHERE contest.id=instance.contest_id AND contest.deleted_at IS NULL
                )
            RETURNING instance.id
            "#,
        )
        .bind(instance)
        .bind(token_hash(&request.client_token))
        .bind(&request.current_view)
        .bind(ip.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("update screen heartbeat", error))?;
        if updated.is_none() {
            return Err(AppError::unauthorized("SCREEN_TOKEN_INVALID", "Screen token is invalid"));
        }
        let command = sqlx::query_as::<_, (i64, String, OffsetDateTime)>("SELECT id,target_view,created_at FROM screen_commands WHERE screen_instance_id=$1 AND acknowledged_at IS NULL ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE")
            .bind(instance).fetch_optional(&mut *tx).await.map_err(|error| AppError::internal("load screen command", error))?;
        if let Some((command_id, _, created_at)) = command.as_ref() {
            sqlx::query("UPDATE screen_commands SET acknowledged_at=now() WHERE screen_instance_id=$1 AND acknowledged_at IS NULL AND (created_at,id) <= ($2,$3)")
                .bind(instance)
                .bind(created_at)
                .bind(command_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| AppError::internal("acknowledge screen commands", error))?;
        }
        let group_playback = playback_for_instance(&mut tx, instance).await?;
        tx.commit().await.map_err(|error| AppError::internal("commit screen heartbeat", error))?;
        Ok(HeartbeatResponse {
            instance_id: instance,
            server_time: OffsetDateTime::now_utc(),
            command_id: command.as_ref().map(|row| row.0),
            target_view: command.map(|row| row.1),
            group_playback,
        })
    }

    pub(super) async fn instances(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<InstanceResponse>, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
        sqlx::query_as("SELECT id,contest_id,name,current_view,(revoked_at IS NULL AND last_seen_at >= now()-interval '45 seconds') AS online,last_seen_at,last_ip,revoked_at,created_at FROM screen_instances WHERE contest_id=$1 ORDER BY created_at,id")
            .bind(contest).fetch_all(&self.database).await.map_err(|error| AppError::internal("list screen instances", error))
    }

    pub(super) async fn command(
        &self,
        contest: i64,
        instance: i64,
        request: CommandRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<CommandResponse, AppError> {
        require_screen_operator(actor)?;
        require_contest(&self.database, contest).await?;
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

    pub(super) async fn revoke(
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
        let changed = sqlx::query(
            r#"
            UPDATE screen_instances instance
            SET revoked_at=coalesce(revoked_at,now()),updated_at=now()
            WHERE instance.id=$1 AND instance.contest_id=$2
                AND EXISTS (
                    SELECT 1 FROM contests contest
                    WHERE contest.id=instance.contest_id AND contest.deleted_at IS NULL
                )
            "#,
        )
        .bind(instance)
        .bind(contest)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("revoke screen instance", error))?
        .rows_affected();
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

pub(super) fn validate_mode(mode: &str) -> Result<&'static str, AppError> {
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

pub(super) fn validate_template(template: &str) -> Result<&str, AppError> {
    match template.trim() {
        "DEFAULT" | "CINEMATIC" | "MINIMAL" | "SPLIT" | "CUSTOM" => Ok(template.trim()),
        _ => Err(AppError::validation(
            "template",
            "must be DEFAULT, CINEMATIC, MINIMAL, SPLIT, or CUSTOM",
        )),
    }
}

pub(super) fn require_presentation_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::SCREEN_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::LIVE_MANAGE)
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "PRESENTATION_PERMISSION_REQUIRED",
            "Presentation management permission is required",
        ))
    }
}
pub(super) fn require_screen_operator(actor: &AuthUser) -> Result<(), AppError> {
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::SCREEN_MANAGE)
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "SCREEN_PERMISSION_REQUIRED",
            "Screen management permission is required",
        ))
    }
}
fn require_mode_operator(actor: &AuthUser, mode: &str) -> Result<(), AppError> {
    if actor.is_super_admin()
        || (mode == "SCREEN"
            && actor.has_permission(crate::features::auth::permissions::SCREEN_MANAGE))
        || (mode == "LIVE" && actor.has_permission(crate::features::auth::permissions::LIVE_MANAGE))
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "PRESENTATION_PERMISSION_REQUIRED",
            "Presentation management permission is required",
        ))
    }
}
pub(super) fn token_hash(token: &str) -> String {
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
