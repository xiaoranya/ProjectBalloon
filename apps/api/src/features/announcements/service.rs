use std::net::IpAddr;

use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::AppError, features::auth::model::AuthUser};

use super::model::{AnnouncementResponse, CreateRequest, UpdateRequest};

pub struct AnnouncementService {
    database: PgPool,
}

impl AnnouncementService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(crate) async fn create(
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

    pub(crate) async fn update_scheduled(
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

    pub(crate) async fn cancel_scheduled(
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

    pub(crate) async fn update(
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

    pub(crate) async fn pin(
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

    pub(crate) async fn withdraw(
        &self,
        id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(), AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin announcement withdrawal", error))?;
        let (contest_id, status, _) = lock_context(&mut tx, id).await?;
        require_manage_tx(&mut tx, contest_id, actor).await?;
        ensure_open_tx(&mut tx, contest_id).await?;
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

    pub(crate) async fn list(
        &self,
        contest_id: i64,
        include_withdrawn: bool,
        actor: &AuthUser,
    ) -> Result<Vec<AnnouncementResponse>, AppError> {
        require_readable(&self.database, contest_id, actor).await?;
        if include_withdrawn {
            require_manage_pool(&self.database, contest_id, actor).await?;
        }
        sqlx::query_as::<_, AnnouncementResponse>(safe_sql!(
            "{ANNOUNCEMENT_SQL} WHERE announcement.contest_id = $1 AND (announcement.status = 'PUBLISHED' OR $2) ORDER BY announcement.pinned DESC, announcement.published_at DESC NULLS LAST, announcement.id DESC LIMIT 1000"
        )).bind(contest_id).bind(include_withdrawn).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list announcements", error))
    }

    pub(crate) async fn get(
        &self,
        id: i64,
        actor: &AuthUser,
    ) -> Result<AnnouncementResponse, AppError> {
        let row = load(&self.database, id).await?;
        require_readable(&self.database, row.contest_id, actor).await?;
        if row.status != "PUBLISHED" {
            require_manage_pool(&self.database, row.contest_id, actor).await?;
        }
        Ok(row)
    }
}

const ANNOUNCEMENT_SQL: &str = r#"
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
    sqlx::query_as::<_, AnnouncementResponse>(safe_sql!(
        "{ANNOUNCEMENT_SQL} WHERE announcement.id = $1"
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
    if actor.is_super_admin() {
        return Ok(());
    }
    if !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE) {
        return Err(not_found());
    }
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE contest_id = $1 AND user_id = $2)",
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
                   EXISTS (SELECT 1 FROM contest_management_assignments a WHERE a.contest_id = contest.id AND a.user_id = $3) OR
                   EXISTS (SELECT 1 FROM team_accounts ta JOIN contest_teams ct ON ct.team_id = ta.team_id WHERE ta.user_id = $3 AND ct.contest_id = contest.id))
        )
    "#).bind(contest_id).bind(actor.is_super_admin()).bind(actor.id).fetch_one(database).await
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

pub(super) async fn schedule_event_tx(
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

fn not_found() -> AppError {
    AppError::not_found("ANNOUNCEMENT_NOT_FOUND", "Announcement was not found")
}
