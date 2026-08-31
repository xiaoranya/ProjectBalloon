use std::net::IpAddr;

use project_balloon_domain::{ContestExtensionError, ContestTransitionError};
use sqlx::{Postgres, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

use crate::features::contests::model::{ContestRow, ContestSchedule};

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

#[cfg(test)]
mod tests {
    use super::{ReadAccess, can_read_all};
    use crate::features::auth::model::{AuthUser, UserType, user_for_test};
    use crate::features::auth::permissions;
    use time::OffsetDateTime;

    #[test]
    fn every_operational_manage_permission_grants_read_all() {
        for permission in [
            permissions::CLARIFICATION_MANAGE,
            permissions::PRINTING_MANAGE,
            permissions::BALLOON_MANAGE,
            permissions::RESOLVER_MANAGE,
            permissions::AWARD_MANAGE,
            permissions::SCREEN_MANAGE,
            permissions::LIVE_MANAGE,
        ] {
            let user = user_for_test(UserType::Staff, &[permission]);
            assert!(can_read_all(&user), "{permission} must grant read-all");
        }
    }

    #[test]
    fn contest_manage_alone_scopes_but_does_not_grant_read_all() {
        let user = user_for_test(UserType::Staff, &[permissions::CONTEST_MANAGE]);
        assert!(!can_read_all(&user));
        let access = ReadAccess::for_user(Some(&user));
        assert!(!access.read_all);
        assert_eq!(access.contest_manager_id, Some(1));
    }

    #[test]
    fn super_admins_read_everything_without_manager_scope() {
        let user = user_for_test(UserType::SuperAdmin, &[]);
        let access = ReadAccess::for_user(Some(&user));
        assert!(access.super_admin);
        assert!(access.read_all);
        assert_eq!(access.contest_manager_id, None);
    }

    #[test]
    fn team_users_get_team_scope_only() {
        let user = user_for_test(UserType::Team, &[]);
        let access = ReadAccess::for_user(Some(&user));
        assert!(!access.read_all);
        assert_eq!(access.team_user_id, Some(1));
    }

    #[test]
    fn anonymous_requests_have_no_access() {
        let access = ReadAccess::for_user(None);
        assert!(!access.super_admin);
        assert!(!access.read_all);
        assert_eq!(access.contest_manager_id, None);
        assert_eq!(access.team_user_id, None);
    }

    #[test]
    fn transition_and_extension_errors_map_to_distinct_codes() {
        use project_balloon_domain::{
            ContestExtensionError, ContestScheduleError, ContestState, ContestTransitionError,
        };

        let transition = |error| super::map_transition_error(error).code().to_owned();
        assert_eq!(
            transition(ContestTransitionError::Invalid {
                from: ContestState::Running,
                to: ContestState::Draft
            }),
            "CONTEST_TRANSITION_INVALID"
        );
        assert_eq!(transition(ContestTransitionError::ScheduleRequired), "VALIDATION_FAILED");
        let invalid_schedule = transition(ContestTransitionError::InvalidSchedule(
            ContestScheduleError::StartAfterFreeze,
        ));
        assert_eq!(invalid_schedule, "VALIDATION_FAILED");

        let extension = |error| super::map_extension_error(error).code().to_owned();
        assert_eq!(
            extension(ContestExtensionError::InvalidState(ContestState::Draft)),
            "CONTEST_EXTENSION_STATUS_INVALID"
        );
        assert_eq!(extension(ContestExtensionError::EndTimeNotSet), "CONTEST_END_TIME_NOT_SET");
        assert_eq!(extension(ContestExtensionError::Stale), "CONTEST_EXTENSION_STALE");
        assert_eq!(extension(ContestExtensionError::NotLater), "CONTEST_EXTENSION_NOT_LATER");
    }

    #[test]
    fn schedule_values_and_timestamp_formatting_cover_edge_inputs() {
        use time::macros::datetime;

        assert_eq!(super::schedule_values(None), (None, None, None));
        let schedule = super::ContestSchedule {
            start_at: datetime!(2026-07-22 08:00 UTC),
            freeze_at: datetime!(2026-07-22 12:00 UTC),
            end_at: datetime!(2026-07-22 16:00 UTC),
        };
        assert_eq!(
            super::schedule_values(Some(schedule)),
            (Some(schedule.start_at), Some(schedule.freeze_at), Some(schedule.end_at))
        );

        assert_eq!(
            super::format_rfc3339(datetime!(2026-07-22 08:00 UTC)).expect("format"),
            "2026-07-22T08:00:00Z"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn contest_write_helpers_persist_state_and_guard_scope(pool: sqlx::PgPool) {
        use crate::features::contests::model::ContestSchedule;
        use std::net::Ipv4Addr;

        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests(name,status,visibility) VALUES('helpers contest','DRAFT','PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("contest");

        let mut tx = pool.begin().await.expect("transaction");
        let locked =
            super::lock_active_contest(&mut tx, contest_id).await.expect("lock active contest");
        assert_eq!(locked.id, contest_id);
        assert_eq!(
            super::lock_active_contest(&mut tx, contest_id + 1)
                .await
                .expect_err("missing contest")
                .code(),
            "CONTEST_NOT_FOUND"
        );

        super::record_audit(&mut tx, 1, "CONTEST_CREATED", contest_id, Ipv4Addr::LOCALHOST.into())
            .await
            .expect("record audit");
        super::record_audit_result(
            &mut tx,
            1,
            "CONTEST_CREATED",
            contest_id,
            Ipv4Addr::LOCALHOST.into(),
            "rejected",
        )
        .await
        .expect("record audit result");
        super::insert_realtime_outbox(&mut tx, contest_id, "CONTESTS_UPDATED", "PUBLIC", "{}")
            .await
            .expect("insert outbox");
        let audit_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE target_id=$1")
                .bind(contest_id.to_string())
                .fetch_one(&mut *tx)
                .await
                .expect("audit count");
        assert_eq!(audit_rows, 2);
        let outbox_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM realtime_outbox WHERE contest_id=$1")
                .bind(contest_id)
                .fetch_one(&mut *tx)
                .await
                .expect("outbox count");
        assert_eq!(outbox_rows, 1);
        tx.commit().await.expect("commit");

        // require_manage: super admins bypass, assigned staff pass, others 404.
        let super_admin = user_for_test(UserType::SuperAdmin, &[]);
        let mut tx = pool.begin().await.expect("transaction");
        assert!(super::require_manage(&mut tx, contest_id, &super_admin).await.is_ok());
        tx.rollback().await.expect("rollback");

        let manager_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('helpers-manager','hash','Helpers Manager','STAFF') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("manager");
        sqlx::query("INSERT INTO contest_management_assignments(user_id,contest_id) VALUES($1,$2)")
            .bind(manager_id)
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("assignment");
        let manager = AuthUser {
            id: manager_id,
            username: "helpers-manager".to_owned(),
            display_name: "Helpers Manager".to_owned(),
            user_type: UserType::Staff,
            permissions: vec![crate::features::auth::permissions::CONTEST_MANAGE.to_owned()],
            password_reset_required: false,
        };
        let mut tx = pool.begin().await.expect("transaction");
        assert!(super::require_manage(&mut tx, contest_id, &manager).await.is_ok());
        tx.rollback().await.expect("rollback");

        let outsider_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('helpers-outsider','hash','Helpers Outsider','STAFF') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("outsider");
        let outsider = AuthUser {
            id: outsider_id,
            username: "helpers-outsider".to_owned(),
            display_name: "Helpers Outsider".to_owned(),
            user_type: UserType::Staff,
            permissions: vec![crate::features::auth::permissions::CONTEST_MANAGE.to_owned()],
            password_reset_required: false,
        };
        let mut tx = pool.begin().await.expect("transaction");
        assert_eq!(
            super::require_manage(&mut tx, contest_id, &outsider)
                .await
                .expect_err("unassigned staff")
                .code(),
            "CONTEST_NOT_FOUND"
        );
        let no_permission = AuthUser { permissions: vec![], ..outsider };
        assert_eq!(
            super::require_manage(&mut tx, contest_id, &no_permission)
                .await
                .expect_err("missing permission")
                .code(),
            "CONTEST_NOT_FOUND"
        );
        tx.rollback().await.expect("rollback");

        // schedule_values stays covered through the write path round trip.
        let mut tx = pool.begin().await.expect("transaction");
        let (start, freeze, end) = super::schedule_values(Some(ContestSchedule {
            start_at: OffsetDateTime::now_utc(),
            freeze_at: OffsetDateTime::now_utc() + time::Duration::minutes(30),
            end_at: OffsetDateTime::now_utc() + time::Duration::HOUR,
        }));
        sqlx::query("UPDATE contests SET start_at=$1,freeze_at=$2,end_at=$3 WHERE id=$4")
            .bind(start)
            .bind(freeze)
            .bind(end)
            .bind(contest_id)
            .execute(&mut *tx)
            .await
            .expect("schedule write");
        let updated = super::lock_active_contest(&mut tx, contest_id).await.expect("reload");
        assert!(updated.start_at.is_some() && updated.end_at.is_some());
        tx.rollback().await.expect("rollback");
    }
}
