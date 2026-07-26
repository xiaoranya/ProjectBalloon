use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    Json,
    extract::{
        ConnectInfo, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{AuthContext, ContestManagerContext, model::AuthUser},
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRequest {
    title: String,
    body: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default, with = "time::serde::rfc3339::option")]
    scheduled_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRequest {
    title: Option<String>,
    body: Option<String>,
    pinned: Option<bool>,
    expected_version: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinRequest {
    pinned: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListQuery {
    #[serde(default)]
    include_withdrawn: bool,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementResponse {
    pub id: i64,
    pub contest_id: i64,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub status: String,
    pub created_by_user_id: i64,
    #[serde(with = "time::serde::rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub scheduled_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub withdrawn_at: Option<OffsetDateTime>,
    pub withdrawn_by_user_id: Option<i64>,
    pub source_clarification_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub cancelled_at: Option<OffsetDateTime>,
    pub cancelled_by_user_id: Option<i64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    pub version: i32,
}

pub struct AnnouncementService {
    database: PgPool,
}

impl AnnouncementService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    async fn create(
        &self,
        contest_id: i64,
        request: CreateRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let (title, body) = validate_text(request.title, request.body)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement create", error))?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
        validate_schedule_tx(&mut tx, contest_id, request.scheduled_at).await?;
        let scheduled = request.scheduled_at.is_some();
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO announcements
                (contest_id, title, body, pinned, status, created_by, published_at, scheduled_at)
            VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $5 = 'PUBLISHED' THEN now() END, $7)
            RETURNING id
        "#,
        )
        .bind(contest_id)
        .bind(title)
        .bind(body)
        .bind(request.pinned)
        .bind(if scheduled { "SCHEDULED" } else { "PUBLISHED" })
        .bind(actor.id)
        .bind(request.scheduled_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("insert announcement", error))?;
        if scheduled {
            audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_SCHEDULED", id, ip).await?;
            schedule_event_tx(&mut tx, contest_id, id, "SCHEDULED").await?;
        } else {
            audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_PUBLISHED", id, ip).await?;
            public_event_tx(&mut tx, contest_id, id, "PUBLISHED").await?;
        }
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit announcement create", error))?;
        load(&self.database, id).await
    }

    async fn update_scheduled(
        &self,
        id: i64,
        request: CreateRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let (title, body) = validate_text(request.title, request.body)?;
        let scheduled_at = request.scheduled_at.ok_or_else(|| {
            AppError::bad_request(
                "ANNOUNCEMENT_SCHEDULE_REQUIRED",
                "scheduledAt is required when scheduling an announcement",
            )
        })?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement reschedule", error))?;
        let (contest_id, status, _) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
        if status != "SCHEDULED" {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_NOT_SCHEDULED",
                "Only a scheduled announcement can be rescheduled",
            ));
        }
        validate_schedule_tx(&mut tx, contest_id, Some(scheduled_at)).await?;
        sqlx::query(
            "UPDATE announcements SET title=$2,body=$3,pinned=$4,scheduled_at=$5,updated_at=now(),version=version+1 WHERE id=$1",
        )
        .bind(id)
        .bind(title)
        .bind(body)
        .bind(request.pinned)
        .bind(scheduled_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("reschedule announcement", error))?;
        audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_SCHEDULE_UPDATED", id, ip).await?;
        schedule_event_tx(&mut tx, contest_id, id, "SCHEDULED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit announcement reschedule", error))?;
        load(&self.database, id).await
    }

    async fn cancel_scheduled(
        &self,
        id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let mut tx = self.database.begin().await.map_err(|error| {
            AppError::internal("begin scheduled announcement cancellation", error)
        })?;
        let (contest_id, status, _) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
        if status != "SCHEDULED" {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_NOT_SCHEDULED",
                "Only a scheduled announcement can be cancelled",
            ));
        }
        sqlx::query(
            "UPDATE announcements SET status='CANCELLED',pinned=false,cancelled_at=now(),cancelled_by=$2,updated_at=now(),version=version+1 WHERE id=$1",
        )
        .bind(id)
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("cancel scheduled announcement", error))?;
        audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_SCHEDULE_CANCELLED", id, ip).await?;
        schedule_event_tx(&mut tx, contest_id, id, "CANCELLED").await?;
        tx.commit().await.map_err(|error| {
            AppError::internal("commit scheduled announcement cancellation", error)
        })?;
        load(&self.database, id).await
    }

    async fn update(
        &self,
        id: i64,
        mut request: UpdateRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        if request.expected_version < 0
            || (request.title.is_none() && request.body.is_none() && request.pinned.is_none())
        {
            return Err(AppError::validation(
                "request",
                "must contain changes and a valid expectedVersion",
            ));
        }
        request.title = request.title.map(|value| value.trim().to_owned());
        request.body = request.body.map(|value| value.trim().to_owned());
        if request
            .title
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.chars().count() > 255)
        {
            return Err(AppError::validation("title", "must contain 1 to 255 characters"));
        }
        if request
            .body
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.chars().count() > 16000)
        {
            return Err(AppError::validation("body", "must contain 1 to 16000 characters"));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement update", error))?;
        let (contest_id, status, version) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
        if status != "PUBLISHED" {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_NOT_PUBLISHED",
                "Only a published announcement can be edited",
            ));
        }
        if version != request.expected_version {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_VERSION_STALE",
                "Announcement changed; reload and retry",
            ));
        }
        sqlx::query("UPDATE announcements SET title = coalesce($2, title), body = coalesce($3, body), pinned = coalesce($4, pinned), updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(request.title).bind(request.body).bind(request.pinned)
            .execute(&mut *tx).await.map_err(|error| AppError::internal("update announcement", error))?;
        audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_UPDATED", id, ip).await?;
        public_event_tx(&mut tx, contest_id, id, "PUBLISHED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit announcement update", error))?;
        load(&self.database, id).await
    }

    async fn pin(
        &self,
        id: i64,
        pinned: bool,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement pin", error))?;
        let (contest_id, status, _) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
        if status != "PUBLISHED" {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_NOT_PUBLISHED",
                "Only a published announcement can be pinned",
            ));
        }
        sqlx::query("UPDATE announcements SET pinned = $2, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(pinned).execute(&mut *tx).await
            .map_err(|error| AppError::internal("pin announcement", error))?;
        audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_PINNED", id, ip).await?;
        public_event_tx(&mut tx, contest_id, id, "PUBLISHED").await?;
        tx.commit().await.map_err(|error| AppError::internal("commit announcement pin", error))?;
        load(&self.database, id).await
    }

    async fn withdraw(&self, id: i64, actor: &AuthUser, ip: IpAddr) -> Result<(), AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement withdrawal", error))?;
        let (contest_id, status, _) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        if status != "PUBLISHED" {
            return Err(AppError::conflict(
                "ANNOUNCEMENT_NOT_PUBLISHED",
                "Only a published announcement can be withdrawn",
            ));
        }
        sqlx::query("UPDATE announcements SET status = 'WITHDRAWN', pinned = false, withdrawn_at = now(), withdrawn_by = $2, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(actor.id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("withdraw announcement", error))?;
        audit_tx(&mut tx, actor.id, "ANNOUNCEMENT_WITHDRAWN", id, ip).await?;
        public_event_tx(&mut tx, contest_id, id, "WITHDRAWN").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit announcement withdrawal", error))
    }

    async fn list(
        &self,
        contest_id: i64,
        include_withdrawn: bool,
        actor: &AuthUser,
    ) -> Result<Vec<AnnouncementResponse>, AppError> {
        require_readable(&self.database, contest_id, actor).await?;
        if include_withdrawn {
            require_manage_pool(&self.database, contest_id, actor).await?;
        }
        sqlx::query_as::<_, AnnouncementResponse>(&format!(
            "{SELECT_COLUMNS} WHERE announcement.contest_id = $1 AND (announcement.status = 'PUBLISHED' OR $2) ORDER BY announcement.pinned DESC, announcement.published_at DESC NULLS LAST, announcement.id DESC LIMIT 1000"
        )).bind(contest_id).bind(include_withdrawn).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list announcements", error))
    }

    async fn get(&self, id: i64, actor: &AuthUser) -> Result<AnnouncementResponse, AppError> {
        let row = load(&self.database, id).await?;
        require_readable(&self.database, row.contest_id, actor).await?;
        if row.status != "PUBLISHED" {
            require_manage_pool(&self.database, row.contest_id, actor).await?;
        }
        Ok(row)
    }
}

const SELECT_COLUMNS: &str = r#"
    SELECT announcement.id, announcement.contest_id, announcement.title, announcement.body,
           announcement.pinned, announcement.status,
           announcement.created_by AS created_by_user_id, announcement.published_at,
           announcement.scheduled_at, announcement.withdrawn_at,
           announcement.withdrawn_by AS withdrawn_by_user_id,
           announcement.source_clarification_id, announcement.cancelled_at,
           announcement.cancelled_by AS cancelled_by_user_id, announcement.created_at,
           announcement.updated_at, announcement.version FROM announcements announcement
"#;

pub(crate) async fn load(database: &PgPool, id: i64) -> Result<AnnouncementResponse, AppError> {
    if id <= 0 {
        return Err(not_found());
    }
    sqlx::query_as::<_, AnnouncementResponse>(&format!(
        "{SELECT_COLUMNS} WHERE announcement.id = $1"
    ))
    .bind(id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load announcement", error))?
    .ok_or_else(not_found)
}

async fn lock_context(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<(i64, String, i32), AppError> {
    sqlx::query_as("SELECT contest_id, status, version FROM announcements WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| AppError::internal("lock announcement", error))?
        .ok_or_else(not_found)
}

pub(crate) fn validate_text(
    mut title: String,
    mut body: String,
) -> Result<(String, String), AppError> {
    title = title.trim().to_owned();
    body = body.trim().to_owned();
    if title.is_empty() || title.chars().count() > 255 {
        return Err(AppError::validation("title", "must contain 1 to 255 characters"));
    }
    if body.is_empty() || body.chars().count() > 16000 {
        return Err(AppError::validation("body", "must contain 1 to 16000 characters"));
    }
    Ok((title, body))
}

pub(crate) async fn ensure_open_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
) -> Result<(), AppError> {
    let open = sqlx::query_scalar::<_, bool>(
        "SELECT status IN ('RUNNING', 'PAUSED') FROM contests WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(contest_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| AppError::internal("load announcement contest", error))?
    .ok_or_else(not_found)?;
    if open {
        Ok(())
    } else {
        Err(AppError::conflict(
            "ANNOUNCEMENT_CONTEST_NOT_OPEN",
            "Announcements can only be changed while a contest is running or paused",
        ))
    }
}

async fn validate_schedule_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    scheduled_at: Option<OffsetDateTime>,
) -> Result<(), AppError> {
    let Some(scheduled_at) = scheduled_at else {
        return Ok(());
    };
    let (end_at, database_now) = sqlx::query_as::<_, (OffsetDateTime, OffsetDateTime)>(
        "SELECT end_at,now() FROM contests WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(contest_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| AppError::internal("load announcement scheduling window", error))?
    .ok_or_else(not_found)?;
    if scheduled_at <= database_now {
        return Err(AppError::bad_request(
            "ANNOUNCEMENT_SCHEDULE_NOT_FUTURE",
            "scheduledAt must be in the future",
        ));
    }
    if scheduled_at > end_at {
        return Err(AppError::bad_request(
            "ANNOUNCEMENT_SCHEDULE_AFTER_CONTEST",
            "scheduledAt must not be after the contest end time",
        ));
    }
    Ok(())
}

pub(crate) async fn require_manage_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    if !actor.has_role("CONTEST_ADMIN") {
        return Err(not_found());
    }
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id = $1 AND user_id = $2)",
    ).bind(contest_id).bind(actor.id).fetch_one(&mut **tx).await
        .map_err(|error| AppError::internal("check announcement management scope", error))?;
    if allowed { Ok(()) } else { Err(not_found()) }
}

async fn require_manage_pool(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let mut tx = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin announcement scope", error))?;
    require_manage_tx(&mut tx, contest_id, actor).await?;
    tx.commit().await.map_err(|error| AppError::internal("commit announcement scope", error))
}

async fn require_readable(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let readable = sqlx::query_scalar::<_, bool>(r#"
        SELECT EXISTS (
            SELECT 1 FROM contests contest WHERE contest.id = $1 AND contest.deleted_at IS NULL
              AND (contest.visibility = 'PUBLIC' OR $2 OR
                   EXISTS (SELECT 1 FROM contest_admin_assignments a WHERE a.contest_id = contest.id AND a.user_id = $3) OR
                   EXISTS (SELECT 1 FROM team_accounts ta JOIN contest_teams ct ON ct.team_id = ta.team_id WHERE ta.user_id = $3 AND ct.contest_id = contest.id))
        )
    "#).bind(contest_id).bind(actor.has_role("SUPER_ADMIN")).bind(actor.id).fetch_one(database).await
        .map_err(|error| AppError::internal("check announcement visibility", error))?;
    if readable { Ok(()) } else { Err(not_found()) }
}

pub(crate) async fn audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    action: &str,
    id: i64,
    ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'ANNOUNCEMENT', $3, $4, 'success')")
        .bind(actor_id).bind(action).bind(id.to_string()).bind(ip.to_string()).execute(&mut **tx).await
        .map(|_| ()).map_err(|error| AppError::internal("record announcement audit", error))
}

pub(crate) async fn public_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    id: i64,
    status: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'ANNOUNCEMENT_UPDATED', 'PUBLIC', $3)")
        .bind(Uuid::new_v4()).bind(contest_id).bind(json!({"announcementId": id, "status": status}))
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("enqueue announcement event", error))
}

async fn schedule_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    id: i64,
    status: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO realtime_outbox (event_id,contest_id,event_type,scope,payload_json) VALUES ($1,$2,'ANNOUNCEMENT_SCHEDULE_UPDATED','STAFF',$3)")
        .bind(Uuid::new_v4())
        .bind(contest_id)
        .bind(json!({"announcementId": id, "status": status}))
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("enqueue announcement schedule event", error))
}

pub struct AnnouncementScheduleRunner {
    database: PgPool,
    poll_interval: Duration,
}

impl AnnouncementScheduleRunner {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database, poll_interval: Duration::from_secs(1) }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => match self.publish_due().await {
                    Ok(changed) if changed > 0 => info!(changed, "processed scheduled announcements"),
                    Ok(_) => {}
                    Err(error) => error!(?error, "scheduled announcement processing failed"),
                },
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                }
            }
        }
    }

    pub async fn publish_due(&self) -> Result<u64, AppError> {
        let mut tx = self.database.begin().await.map_err(|error| {
            AppError::internal("begin scheduled announcement processing", error)
        })?;
        let due = sqlx::query_as::<_, (i64, i64, i64, String)>(
            r#"
            SELECT announcement.id,announcement.contest_id,announcement.created_by,contest.status
            FROM announcements announcement
            JOIN contests contest ON contest.id=announcement.contest_id
            WHERE announcement.status='SCHEDULED' AND announcement.scheduled_at<=now()
            ORDER BY announcement.scheduled_at,announcement.id
            FOR UPDATE OF announcement,contest SKIP LOCKED LIMIT 100
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load due scheduled announcements", error))?;
        for (id, contest_id, created_by, contest_status) in &due {
            if matches!(contest_status.as_str(), "RUNNING" | "PAUSED") {
                sqlx::query("UPDATE announcements SET status='PUBLISHED',published_at=now(),updated_at=now(),version=version+1 WHERE id=$1 AND status='SCHEDULED'")
                    .bind(id).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("publish scheduled announcement", error))?;
                automatic_audit_tx(&mut tx, *created_by, "ANNOUNCEMENT_PUBLISHED", *id).await?;
                public_event_tx(&mut tx, *contest_id, *id, "PUBLISHED").await?;
            } else {
                sqlx::query("UPDATE announcements SET status='CANCELLED',pinned=false,cancelled_at=now(),cancelled_by=$2,updated_at=now(),version=version+1 WHERE id=$1 AND status='SCHEDULED'")
                    .bind(id).bind(created_by).execute(&mut *tx).await
                    .map_err(|error| AppError::internal("cancel expired scheduled announcement", error))?;
                automatic_audit_tx(&mut tx, *created_by, "ANNOUNCEMENT_SCHEDULE_CANCELLED", *id)
                    .await?;
                schedule_event_tx(&mut tx, *contest_id, *id, "CANCELLED").await?;
            }
        }
        tx.commit().await.map_err(|error| {
            AppError::internal("commit scheduled announcement processing", error)
        })?;
        Ok(due.len() as u64)
    }
}

async fn automatic_audit_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    action: &str,
    id: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id,action,target_type,target_id,request_ip,result) VALUES ($1,$2,'ANNOUNCEMENT',$3,'system','success')")
        .bind(actor_id)
        .bind(action)
        .bind(id.to_string())
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("record automatic announcement audit", error))
}

fn not_found() -> AppError {
    AppError::not_found("ANNOUNCEMENT_NOT_FOUND", "Announcement was not found")
}

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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;
    use time::OffsetDateTime;

    use super::{
        AnnouncementScheduleRunner, AnnouncementService, CreateRequest, UpdateRequest,
        validate_text,
    };
    use crate::features::auth::model::{AuthUser, UserType};

    #[test]
    fn announcement_text_is_trimmed_and_bounded() {
        assert_eq!(
            validate_text(" Title ".into(), " Body ".into()).expect("valid text"),
            ("Title".into(), "Body".into())
        );
        assert!(validate_text(" ".into(), "Body".into()).is_err());
        assert!(validate_text("Title".into(), "x".repeat(16_001)).is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn published_announcement_is_editable_pinnable_and_irreversibly_withdrawn(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('ann-root', 'test-hash', 'Ann Root', 'SUPER_ADMIN') RETURNING id",
        ).fetch_one(&pool).await.expect("insert announcement administrator");
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('ann-team', 'test-hash', 'Ann Team', 'TEAM') RETURNING id",
        ).fetch_one(&pool).await.expect("insert announcement team user");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Ann Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert announcement team");
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(team_id)
            .execute(&pool)
            .await
            .expect("link announcement team");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, end_at)
            VALUES ('Announcement Contest', 'RUNNING', 'PRIVATE',
                    now() - interval '1 hour', now() + interval '1 hour') RETURNING id
        "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert announcement contest");
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id).bind(team_id).execute(&pool).await.expect("roster announcement team");
        let admin = AuthUser {
            id: admin_id,
            username: "ann-root".into(),
            display_name: "Ann Root".into(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let team = AuthUser {
            id: user_id,
            username: "ann-team".into(),
            display_name: "Ann Team".into(),
            user_type: UserType::Team,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let service = AnnouncementService::new(pool.clone());
        let created = service
            .create(
                contest_id,
                CreateRequest {
                    title: "Notice".into(),
                    body: "Initial".into(),
                    pinned: false,
                    scheduled_at: None,
                },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("publish announcement");
        assert_eq!(service.list(contest_id, false, &team).await.expect("team list").len(), 1);
        let updated = service
            .update(
                created.id,
                UpdateRequest {
                    title: None,
                    body: Some("Updated".into()),
                    pinned: None,
                    expected_version: 0,
                },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("edit announcement");
        assert_eq!(updated.body, "Updated");
        assert_eq!(updated.version, 1);
        let pinned = service
            .pin(created.id, true, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("pin announcement");
        assert!(pinned.pinned);
        service
            .withdraw(created.id, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("withdraw announcement");
        assert!(
            service
                .list(contest_id, false, &team)
                .await
                .expect("team list after withdrawal")
                .is_empty()
        );
        let history = service.list(contest_id, true, &admin).await.expect("administrator history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "WITHDRAWN");
        assert!(
            service.pin(created.id, false, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST)).await.is_err()
        );
        let public_events = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM realtime_outbox WHERE contest_id = $1 AND event_type = 'ANNOUNCEMENT_UPDATED' AND scope = 'PUBLIC'",
        ).bind(contest_id).fetch_one(&pool).await.expect("count announcement events");
        assert_eq!(public_events, 4);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn scheduled_announcements_can_be_changed_cancelled_and_published_once(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username,password_hash,display_name,user_type) VALUES ('schedule-root','test-hash','Schedule Root','SUPER_ADMIN') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert administrator");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name,status,visibility,start_at,end_at) VALUES ('Scheduled Announcements','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '2 hours') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let admin = AuthUser {
            id: admin_id,
            username: "schedule-root".into(),
            display_name: "Schedule Root".into(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let service = AnnouncementService::new(pool.clone());
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let scheduled = service
            .create(
                contest_id,
                CreateRequest {
                    title: "Later".into(),
                    body: "First draft".into(),
                    pinned: true,
                    scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(10)),
                },
                &admin,
                ip,
            )
            .await
            .expect("schedule announcement");
        assert_eq!(scheduled.status, "SCHEDULED");
        assert!(scheduled.published_at.is_none());
        assert!(service.list(contest_id, false, &admin).await.expect("public list").is_empty());

        let changed = service
            .update_scheduled(
                scheduled.id,
                CreateRequest {
                    title: "Later updated".into(),
                    body: "Second draft".into(),
                    pinned: false,
                    scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(20)),
                },
                &admin,
                ip,
            )
            .await
            .expect("reschedule announcement");
        assert_eq!(changed.version, 1);
        assert_eq!(changed.title, "Later updated");
        let cancelled =
            service.cancel_scheduled(scheduled.id, &admin, ip).await.expect("cancel schedule");
        assert_eq!(cancelled.status, "CANCELLED");
        assert!(!cancelled.pinned);

        let due = service
            .create(
                contest_id,
                CreateRequest {
                    title: "Due".into(),
                    body: "Publish me".into(),
                    pinned: false,
                    scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(5)),
                },
                &admin,
                ip,
            )
            .await
            .expect("schedule due announcement");
        sqlx::query("UPDATE announcements SET scheduled_at=now()-interval '1 second' WHERE id=$1")
            .bind(due.id)
            .execute(&pool)
            .await
            .expect("make announcement due");
        let first = AnnouncementScheduleRunner::new(pool.clone());
        let second = AnnouncementScheduleRunner::new(pool.clone());
        let (a, b) = tokio::join!(first.publish_due(), second.publish_due());
        assert_eq!(a.expect("first runner") + b.expect("second runner"), 1);
        let published = super::load(&pool, due.id).await.expect("load published announcement");
        assert_eq!(published.status, "PUBLISHED");
        assert!(published.published_at.is_some());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM audit_logs WHERE target_type='ANNOUNCEMENT' AND target_id=$1 AND action='ANNOUNCEMENT_PUBLISHED'",
            )
            .bind(due.id.to_string())
            .fetch_one(&pool)
            .await
            .expect("count publish audits"),
            1
        );

        let expired = service
            .create(
                contest_id,
                CreateRequest {
                    title: "Expired".into(),
                    body: "Do not publish".into(),
                    pinned: true,
                    scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(5)),
                },
                &admin,
                ip,
            )
            .await
            .expect("schedule expiring announcement");
        sqlx::query("UPDATE announcements SET scheduled_at=now()-interval '1 second' WHERE id=$1")
            .bind(expired.id)
            .execute(&pool)
            .await
            .expect("make expired announcement due");
        sqlx::query("UPDATE contests SET status='ENDED' WHERE id=$1")
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("end contest");
        assert_eq!(
            AnnouncementScheduleRunner::new(pool.clone())
                .publish_due()
                .await
                .expect("cancel expired schedule"),
            1
        );
        let expired = super::load(&pool, expired.id).await.expect("load expired announcement");
        assert_eq!(expired.status, "CANCELLED");
        assert!(expired.cancelled_at.is_some());
    }
}
