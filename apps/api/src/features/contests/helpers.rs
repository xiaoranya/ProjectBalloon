use std::net::IpAddr;

use project_balloon_domain::{ContestExtensionError, ContestTransitionError};
use sqlx::{Postgres, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

use super::model::{ContestRow, ContestSchedule};

pub(super) const CONTEST_COLUMNS: &str = r#"
    id,
    name,
    status,
    visibility,
    start_at,
    end_at,
    freeze_at,
    version,
    created_at,
    updated_at,
    deleted_at
"#;

pub(super) struct ReadAccess {
    pub(super) super_admin: bool,
    pub(super) read_all: bool,
    pub(super) contest_manager_id: Option<i64>,
    pub(super) team_user_id: Option<i64>,
}

impl ReadAccess {
    pub(super) fn for_user(user: Option<&AuthUser>) -> Self {
        let super_admin = user.is_some_and(|user| user.is_super_admin());
        let read_all = user.is_some_and(can_read_all);
        let contest_manager_id = user
            .filter(|user| {
                !user.is_super_admin()
                    && user.has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
            })
            .map(|user| user.id);
        let team_user_id = user.filter(|user| user.user_type == UserType::Team).map(|user| user.id);
        Self { super_admin, read_all, contest_manager_id, team_user_id }
    }
}

fn can_read_all(user: &AuthUser) -> bool {
    user.is_super_admin()
        || [
            crate::features::auth::permissions::CLARIFICATION_MANAGE,
            crate::features::auth::permissions::PRINTING_MANAGE,
            crate::features::auth::permissions::BALLOON_MANAGE,
            crate::features::auth::permissions::RESOLVER_MANAGE,
            crate::features::auth::permissions::AWARD_MANAGE,
            crate::features::auth::permissions::SCREEN_MANAGE,
            crate::features::auth::permissions::LIVE_MANAGE,
        ]
        .iter()
        .any(|permission| user.has_permission(permission))
}

pub(super) async fn require_manage(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    if !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE) {
        return Err(contest_not_found());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_management_assignments
            WHERE user_id = $1 AND contest_id = $2
        )
        "#,
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check contest manager scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

pub(super) async fn lock_active_contest(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
) -> Result<ContestRow, AppError> {
    let sql = format!(
        r#"
        SELECT {CONTEST_COLUMNS}
        FROM contests
        WHERE id = $1 AND deleted_at IS NULL
        FOR UPDATE
        "#
    );
    sqlx::query_as::<_, ContestRow>(sqlx::AssertSqlSafe(sql))
        .bind(contest_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("lock active contest", error))?
        .ok_or_else(contest_not_found)
}

pub(super) async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    contest_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    record_audit_result(transaction, actor_user_id, action, contest_id, request_ip, "success").await
}

pub(super) async fn record_audit_result(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    contest_id: i64,
    request_ip: IpAddr,
    result: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES
            ($1, $2, 'CONTEST', $3, $4, $5)
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(contest_id.to_string())
    .bind(request_ip.to_string())
    .bind(result)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record contest audit", error))
}

pub(super) async fn insert_realtime_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    event_type: &'static str,
    scope: &'static str,
    payload_json: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO realtime_outbox
            (event_id, contest_id, event_type, scope, payload_json)
        VALUES
            ($1, $2, $3, $4, $5::jsonb)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(contest_id)
    .bind(event_type)
    .bind(scope)
    .bind(payload_json)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("persist realtime outbox event", error))
}

pub(super) fn schedule_values(
    schedule: Option<ContestSchedule>,
) -> (Option<time::OffsetDateTime>, Option<time::OffsetDateTime>, Option<time::OffsetDateTime>) {
    schedule.map_or((None, None, None), |schedule| {
        (Some(schedule.start_at), Some(schedule.freeze_at), Some(schedule.end_at))
    })
}

pub(super) fn map_contest_write_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("idx_contests_active_name_unique")
    {
        AppError::conflict("CONTEST_NAME_TAKEN", "Contest name is already in use")
    } else {
        AppError::internal("write contest", error)
    }
}

pub(super) fn map_transition_error(error: ContestTransitionError) -> AppError {
    match error {
        ContestTransitionError::Invalid { .. } => AppError::conflict(
            "CONTEST_TRANSITION_INVALID",
            "The requested contest lifecycle transition is not allowed",
        ),
        ContestTransitionError::ScheduleRequired => AppError::validation(
            "schedule",
            "must be configured before freezing contest configuration",
        ),
        ContestTransitionError::InvalidSchedule(_) => {
            AppError::validation("schedule", "must be ordered before freezing configuration")
        }
    }
}

pub(super) fn map_extension_error(error: ContestExtensionError) -> AppError {
    match error {
        ContestExtensionError::InvalidState(_) => AppError::conflict(
            "CONTEST_EXTENSION_STATUS_INVALID",
            "Contest can be extended only while running or paused",
        ),
        ContestExtensionError::EndTimeNotSet => {
            AppError::conflict("CONTEST_END_TIME_NOT_SET", "Contest end time is not configured")
        }
        ContestExtensionError::Stale => AppError::conflict(
            "CONTEST_EXTENSION_STALE",
            "Contest end time changed; reload before extending",
        ),
        ContestExtensionError::NotLater => AppError::conflict(
            "CONTEST_EXTENSION_NOT_LATER",
            "New contest end time must be later than the current end time",
        ),
    }
}

pub(super) fn format_rfc3339(value: OffsetDateTime) -> Result<String, AppError> {
    value
        .format(&Rfc3339)
        .map_err(|error| AppError::internal("format realtime event timestamp", error))
}

pub(super) fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}
