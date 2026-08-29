use std::net::IpAddr;

use crate::error::AppError;
use crate::features::auth::model::AuthUser;

use crate::features::teams::model::{
    TeamMemberResponse, ValidatedTeamMember, ValidatedTeamMemberPatch,
};
use crate::features::teams::service::TeamService;
use crate::features::teams::service::helpers::{record_audit, require_manage_team};

impl TeamService {
    pub async fn add_member(
        &self,
        team_id: i64,
        request: ValidatedTeamMember,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<TeamMemberResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team member creation", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let member = sqlx::query_as(
            r#"
            INSERT INTO team_members (team_id, name, email, phone, role_name)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, team_id, name, email, phone, role_name, created_at
            "#,
        )
        .bind(team_id)
        .bind(request.name)
        .bind(request.email)
        .bind(request.phone)
        .bind(request.role_name)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("create team member", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_MEMBER_CREATED",
            "team_member",
            &member_id(&member).to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team member creation", error))?;
        Ok(member)
    }

    pub async fn list_members(
        &self,
        team_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<TeamMemberResponse>, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team member list", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let members = sqlx::query_as(
            r#"
            SELECT id, team_id, name, email, phone, role_name, created_at
            FROM team_members
            WHERE team_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(team_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("list team members", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team member list", error))?;
        Ok(members)
    }

    pub async fn update_member(
        &self,
        team_id: i64,
        member_id_value: i64,
        request: ValidatedTeamMemberPatch,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<TeamMemberResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team member update", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let member = sqlx::query_as(
            r#"
            UPDATE team_members
            SET name = COALESCE($1, name),
                email = COALESCE($2, email),
                phone = COALESCE($3, phone),
                role_name = COALESCE($4, role_name)
            WHERE id = $5 AND team_id = $6
            RETURNING id, team_id, name, email, phone, role_name, created_at
            "#,
        )
        .bind(request.name)
        .bind(request.email)
        .bind(request.phone)
        .bind(request.role_name)
        .bind(member_id_value)
        .bind(team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("update team member", error))?
        .ok_or_else(team_member_not_found)?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_MEMBER_UPDATED",
            "team_member",
            &member_id_value.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team member update", error))?;
        Ok(member)
    }

    pub async fn remove_member(
        &self,
        team_id: i64,
        member_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team member deletion", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let result = sqlx::query("DELETE FROM team_members WHERE id = $1 AND team_id = $2")
            .bind(member_id)
            .bind(team_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("delete team member", error))?;
        if result.rows_affected() != 1 {
            return Err(team_member_not_found());
        }
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_MEMBER_DELETED",
            "team_member",
            &member_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team member deletion", error))
    }
}

fn member_id(member: &TeamMemberResponse) -> i64 {
    member.id
}

fn team_member_not_found() -> AppError {
    AppError::not_found("TEAM_MEMBER_NOT_FOUND", "Team member was not found")
}
