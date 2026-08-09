use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

pub(super) async fn lock_configurable_contest(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM contests WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(contest_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("lock contest problem configuration", error))?
    .ok_or_else(contest_not_found)?;
    require_manage_transaction(transaction, contest_id, actor).await?;
    if status != "DRAFT" {
        return Err(AppError::conflict(
            "CONTEST_PROBLEM_CONFIG_FROZEN",
            "Contest problem configuration can be changed only in DRAFT",
        ));
    }
    Ok(())
}

pub(super) async fn require_readable(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM contests WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(contest_id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("check readable contest problems", error))?
    .ok_or_else(contest_not_found)?;

    if actor.user_type == UserType::Team {
        if !matches!(status.as_str(), "RUNNING" | "PAUSED" | "ENDED" | "ARCHIVED") {
            return Err(contest_not_found());
        }
        let participating = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM team_accounts account
                JOIN contest_teams roster ON roster.team_id = account.team_id
                WHERE account.user_id = $1 AND roster.contest_id = $2
            )
            "#,
        )
        .bind(actor.id)
        .bind(contest_id)
        .fetch_one(database)
        .await
        .map_err(|error| AppError::internal("check team contest problem access", error))?;
        return if participating { Ok(()) } else { Err(contest_not_found()) };
    }

    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::CLARIFICATION_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::PRINTING_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::BALLOON_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::RESOLVER_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::AWARD_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::SCREEN_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::LIVE_MANAGE)
    {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE user_id = $1 AND contest_id = $2)",
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check contest problem read scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

pub(super) async fn require_manage_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE user_id = $1 AND contest_id = $2)",
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check contest problem mutation scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

pub(super) async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    contest_id: i64,
    problem_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES ($1, $2, 'CONTEST_PROBLEM', $3, $4, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(format!("{contest_id}:{problem_id}"))
    .bind(request_ip.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record contest problem audit", error))
}

pub(super) async fn record_reorder_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    contest_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES ($1, 'CONTEST_PROBLEMS_REORDERED', 'CONTEST', $2, $3, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(contest_id.to_string())
    .bind(request_ip.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record contest problem reorder audit", error))
}

pub(super) fn require_positive_contest_id(contest_id: i64) -> Result<(), AppError> {
    if contest_id > 0 { Ok(()) } else { Err(AppError::validation("contestId", "must be positive")) }
}

pub(super) fn require_ids(contest_id: i64, problem_id: i64) -> Result<(), AppError> {
    require_positive_contest_id(contest_id)?;
    if problem_id > 0 { Ok(()) } else { Err(AppError::validation("problemId", "must be positive")) }
}

pub(super) fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

pub(super) fn assignment_not_found() -> AppError {
    AppError::not_found("CONTEST_PROBLEM_NOT_FOUND", "Contest problem assignment was not found")
}

pub(super) fn map_assignment_write_error(error: sqlx::Error) -> AppError {
    match error.as_database_error().and_then(sqlx::error::DatabaseError::constraint) {
        Some("contest_problems_pkey") => AppError::conflict(
            "PROBLEM_ALREADY_ASSIGNED",
            "Problem is already assigned to this contest",
        ),
        Some("contest_problems_contest_id_alias_key") => AppError::conflict(
            "CONTEST_PROBLEM_ALIAS_TAKEN",
            "Problem alias is already used in this contest",
        ),
        Some("contest_problems_contest_id_display_order_key") => AppError::conflict(
            "CONTEST_PROBLEM_ORDER_TAKEN",
            "Display order is already used in this contest",
        ),
        _ => AppError::internal("write contest problem assignment", error),
    }
}
