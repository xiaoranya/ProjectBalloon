use std::net::IpAddr;

use serde_json::Value;
use sqlx::{Postgres, Transaction};

use crate::error::AppError;
use crate::features::auth::model::AuthUser;

use crate::features::teams::model::{
    ContestTeamResponse, ParticipationType, ValidatedContestTeamAssignment,
};
use crate::features::teams::service::TeamService;
use crate::features::teams::service::helpers::{enqueue_realtime, record_audit, team_not_found};

impl TeamService {
    pub async fn assign_to_contest(
        &self,
        contest_id: i64,
        request: ValidatedContestTeamAssignment,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ContestTeamResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest team assignment", error))?;
        require_manage_contest(&mut transaction, contest_id, actor).await?;
        lock_open_contest(&mut transaction, contest_id).await?;
        let team = sqlx::query_as::<_, (String, bool)>(
            "SELECT name, star FROM teams WHERE id = $1 AND deleted_at IS NULL FOR SHARE",
        )
        .bind(request.team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("load assigned team", error))?
        .ok_or_else(team_not_found)?;
        let participation_type =
            if team.1 { ParticipationType::Star } else { request.participation_type };
        let row = sqlx::query_as(
            r#"
            INSERT INTO contest_teams
                (contest_id, team_id, participation_type, group_name)
            VALUES ($1, $2, $3, $4)
            RETURNING id, contest_id, team_id, $5::text AS team_name,
                      participation_type, group_name, created_at
            "#,
        )
        .bind(contest_id)
        .bind(request.team_id)
        .bind(participation_type.as_str())
        .bind(request.group_name)
        .bind(team.0)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_contest_team_write_error)?;
        roster_changed(
            &mut transaction,
            actor.id,
            contest_id,
            request.team_id,
            "CONTEST_TEAM_ASSIGNED",
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest team assignment", error))?;
        Ok(row)
    }

    pub async fn list_contest_teams(
        &self,
        contest_id: i64,
    ) -> Result<Vec<ContestTeamResponse>, AppError> {
        sqlx::query_as(
            r#"
            SELECT ct.id, ct.contest_id, ct.team_id, t.name AS team_name,
                   ct.participation_type, ct.group_name, ct.created_at
            FROM contest_teams ct
            JOIN teams t ON t.id = ct.team_id
            WHERE ct.contest_id = $1 AND t.deleted_at IS NULL
            ORDER BY ct.created_at, ct.id
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list contest teams", error))
    }

    pub async fn remove_from_contest(
        &self,
        contest_id: i64,
        team_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest team removal", error))?;
        require_manage_contest(&mut transaction, contest_id, actor).await?;
        lock_open_contest(&mut transaction, contest_id).await?;
        let result =
            sqlx::query("DELETE FROM contest_teams WHERE contest_id = $1 AND team_id = $2")
                .bind(contest_id)
                .bind(team_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("remove contest team", error))?;
        if result.rows_affected() != 1 {
            return Err(AppError::not_found(
                "CONTEST_TEAM_NOT_FOUND",
                "Contest team assignment was not found",
            ));
        }
        roster_changed(
            &mut transaction,
            actor.id,
            contest_id,
            team_id,
            "CONTEST_TEAM_UNASSIGNED",
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest team removal", error))
    }
}

pub(super) async fn require_manage_contest(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if contest_id <= 0 {
        return Err(contest_not_found());
    }
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
    .map_err(|error| AppError::internal("check contest team management scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

pub(super) async fn lock_open_contest(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
) -> Result<(), AppError> {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM contests WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(contest_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("lock contest for roster change", error))?
    .ok_or_else(contest_not_found)?;
    if matches!(status.as_str(), "ENDED" | "ARCHIVED") {
        Err(AppError::conflict(
            "CONTEST_ROSTER_CLOSED",
            "Contest roster cannot change after the contest has ended",
        ))
    } else {
        Ok(())
    }
}

async fn roster_changed(
    transaction: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    contest_id: i64,
    team_id: i64,
    action: &'static str,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    record_audit(
        transaction,
        actor_id,
        action,
        "contest_team",
        &format!("{contest_id}:{team_id}"),
        request_ip,
        "success",
    )
    .await?;
    enqueue_realtime(
        transaction,
        contest_id,
        "CONTEST_TEAMS_CHANGED",
        "STAFF",
        None,
        serde_json::json!({"contestId": contest_id, "teamId": team_id, "action": action}),
    )
    .await?;
    enqueue_realtime(
        transaction,
        contest_id,
        "CONTEST_TEAMS_CHANGED",
        "TEAM",
        Some(team_id),
        serde_json::json!({"contestId": contest_id, "action": action}),
    )
    .await
}

pub(super) async fn enqueue_for_team_contests(
    transaction: &mut Transaction<'_, Postgres>,
    team_id: i64,
    event_type: &'static str,
    payload: Value,
) -> Result<(), AppError> {
    let contest_ids =
        sqlx::query_scalar::<_, i64>("SELECT contest_id FROM contest_teams WHERE team_id = $1")
            .bind(team_id)
            .fetch_all(&mut **transaction)
            .await
            .map_err(|error| AppError::internal("list team contests for realtime event", error))?;
    for contest_id in contest_ids {
        enqueue_realtime(transaction, contest_id, event_type, "STAFF", None, payload.clone())
            .await?;
    }
    Ok(())
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

pub(super) fn map_contest_team_write_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("contest_teams_contest_id_team_id_key")
    {
        AppError::conflict(
            "CONTEST_TEAM_ALREADY_ASSIGNED",
            "Team is already assigned to this contest",
        )
    } else {
        AppError::internal("write contest team assignment", error)
    }
}
