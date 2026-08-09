use std::net::IpAddr;

use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{hash_password, model::AuthUser},
};

use super::super::model::ValidatedCreateTeam;

pub(super) async fn prepare_team(request: ValidatedCreateTeam) -> Result<PreparedTeam, AppError> {
    let password_hash = match &request.account {
        Some(account) => Some(
            hash_password(account.initial_password.clone())
                .await
                .map_err(|error| AppError::internal("hash initial team password", error))?,
        ),
        None => None,
    };
    Ok(PreparedTeam { request, password_hash })
}

pub(super) async fn create_team_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    prepared: PreparedTeam,
) -> Result<(i64, Option<(i64, String)>), AppError> {
    let team_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO teams (name, school, seat_no, group_name, star)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(&prepared.request.name)
    .bind(prepared.request.school)
    .bind(prepared.request.seat_no)
    .bind(prepared.request.group_name)
    .bind(prepared.request.star)
    .fetch_one(&mut **transaction)
    .await
    .map_err(map_team_write_error)?;
    let account = match (prepared.request.account, prepared.password_hash) {
        (Some(account), Some(password_hash)) => {
            let user_id = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO users
                    (username, password_hash, display_name, user_type, enabled,
                     password_reset_required)
                VALUES ($1, $2, $3, 'TEAM', true, $4)
                RETURNING id
                "#,
            )
            .bind(&account.username)
            .bind(password_hash)
            .bind(&prepared.request.name)
            .bind(prepared.request.require_password_reset)
            .fetch_one(&mut **transaction)
            .await
            .map_err(map_team_write_error)?;
            sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(team_id)
                .execute(&mut **transaction)
                .await
                .map_err(|error| AppError::internal("link team account", error))?;
            Some((user_id, account.username))
        }
        (None, None) => None,
        _ => return Err(AppError::internal("create team account", "incomplete prepared account")),
    };
    Ok((team_id, account))
}

pub(super) struct PreparedTeam {
    pub(super) request: ValidatedCreateTeam,
    pub(super) password_hash: Option<String>,
}

pub(super) async fn require_manage_team(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    require_positive_team_id(team_id)?;
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM teams WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(team_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check active team", error))?;
    if !exists {
        return Err(team_not_found());
    }
    if actor.is_super_admin() {
        return Ok(());
    }
    if !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE) {
        return Err(team_not_found());
    }
    let (contest_count, unmanaged_count) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            count(*),
            count(*) FILTER (
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM contest_management_assignments caa
                    WHERE caa.user_id = $2 AND caa.contest_id = ct.contest_id
                )
            )
        FROM contest_teams ct
        JOIN contests contest
          ON contest.id = ct.contest_id AND contest.deleted_at IS NULL
        WHERE ct.team_id = $1
        "#,
    )
    .bind(team_id)
    .bind(actor.id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check team administrator scope", error))?;
    if contest_count > 0 && unmanaged_count == 0 { Ok(()) } else { Err(team_not_found()) }
}

pub(super) async fn enqueue_realtime(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    event_type: &'static str,
    scope: &'static str,
    team_id: Option<i64>,
    payload: Value,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO realtime_outbox
            (event_id, contest_id, event_type, scope, team_id, payload_json)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(contest_id)
    .bind(event_type)
    .bind(scope)
    .bind(team_id)
    .bind(payload)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("enqueue team realtime event", error))
}

pub(super) async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    target_type: &'static str,
    target_id: &str,
    request_ip: IpAddr,
    result: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(request_ip.to_string())
    .bind(result)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record team audit", error))
}

pub(super) fn require_positive_team_id(team_id: i64) -> Result<(), AppError> {
    if team_id > 0 { Ok(()) } else { Err(team_not_found()) }
}

pub(super) fn team_not_found() -> AppError {
    AppError::not_found("TEAM_NOT_FOUND", "Team was not found")
}

pub(super) fn map_team_write_error(error: sqlx::Error) -> AppError {
    match error.as_database_error().and_then(sqlx::error::DatabaseError::constraint) {
        Some("idx_teams_active_name_unique") => {
            AppError::conflict("TEAM_NAME_TAKEN", "An active team already uses this name")
        }
        Some("users_username_key") => {
            AppError::conflict("USERNAME_TAKEN", "Username is already in use")
        }
        _ => AppError::internal("write team", error),
    }
}
