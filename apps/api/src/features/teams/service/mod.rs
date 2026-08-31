use std::net::IpAddr;

use sqlx::PgPool;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    pagination::PageResponse,
};

use crate::features::teams::model::{
    TeamResponse, TeamRow, ValidatedCreateTeam, ValidatedTeamListQuery, ValidatedUpdateTeam,
};
use contest_roster::enqueue_for_team_contests;
use helpers::{
    create_team_in_transaction, map_team_write_error, prepare_team, record_audit,
    require_manage_team, require_positive_team_id, team_not_found,
};

mod contest_roster;
mod helpers;
mod import;
mod members;
mod password;
#[cfg(test)]
mod tests;

const TEAM_COLUMNS: &str = r#"
    t.id,
    t.name,
    t.school,
    t.seat_no,
    t.group_name,
    t.star,
    t.version,
    account.user_id AS account_user_id,
    account_user.username AS account_username,
    account_user.enabled AS account_enabled,
    account_user.password_reset_required AS account_password_reset_required,
    t.deleted_at,
    t.created_at,
    t.updated_at
"#;

pub struct TeamService {
    pub(super) database: PgPool,
}

impl TeamService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn create(
        &self,
        request: ValidatedCreateTeam,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<TeamResponse, AppError> {
        let prepared = prepare_team(request).await?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team creation", error))?;
        let (team_id, _) = create_team_in_transaction(&mut transaction, prepared).await?;
        record_audit(
            &mut transaction,
            actor_user_id,
            "TEAM_CREATED",
            "team",
            &team_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team creation", error))?;
        self.load(team_id, false).await
    }

    pub async fn list(
        &self,
        query: ValidatedTeamListQuery,
        actor: &AuthUser,
    ) -> Result<PageResponse<TeamResponse>, AppError> {
        if actor.user_type == UserType::Team {
            return Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"));
        }
        let super_admin = actor.is_super_admin();
        if !super_admin && !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
        {
            return Err(AppError::forbidden(
                "FORBIDDEN",
                "Only team administrators may list teams",
            ));
        }
        if query.include_deleted && !super_admin {
            return Err(AppError::forbidden(
                "FORBIDDEN",
                "Only super administrators may include deleted teams",
            ));
        }
        let contest_manager_id = (!super_admin).then_some(actor.id);
        let include_deleted = query.include_deleted && super_admin;
        let total_elements = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM teams t
            WHERE ($1 OR t.deleted_at IS NULL)
              AND (
                    $2::bigint IS NULL
                    OR EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN contests contest
                          ON contest.id = ct.contest_id AND contest.deleted_at IS NULL
                        JOIN contest_management_assignments caa
                          ON caa.contest_id = ct.contest_id
                        WHERE ct.team_id = t.id AND caa.user_id = $2
                    )
              )
            "#,
        )
        .bind(include_deleted)
        .bind(contest_manager_id)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("count visible teams", error))?;
        let sql = format!(
            r#"
            SELECT {TEAM_COLUMNS}
            FROM teams t
            LEFT JOIN team_accounts account ON account.team_id = t.id
            LEFT JOIN users account_user ON account_user.id = account.user_id
            WHERE ($1 OR t.deleted_at IS NULL)
              AND (
                    $2::bigint IS NULL
                    OR EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN contests contest
                          ON contest.id = ct.contest_id AND contest.deleted_at IS NULL
                        JOIN contest_management_assignments caa
                          ON caa.contest_id = ct.contest_id
                        WHERE ct.team_id = t.id AND caa.user_id = $2
                    )
              )
            ORDER BY {}
            LIMIT $3 OFFSET $4
            "#,
            query.order_by
        );
        let rows = sqlx::query_as::<_, TeamRow>(sqlx::AssertSqlSafe(sql))
            .bind(include_deleted)
            .bind(contest_manager_id)
            .bind(i64::from(query.size))
            .bind(query.offset)
            .fetch_all(&self.database)
            .await
            .map_err(|error| AppError::internal("list teams", error))?;
        let content = rows.into_iter().map(TeamRow::response).collect::<Result<Vec<_>, _>>()?;
        Ok(PageResponse::new(content, query.page, query.size, total_elements))
    }

    pub async fn get(&self, team_id: i64, actor: &AuthUser) -> Result<TeamResponse, AppError> {
        require_positive_team_id(team_id)?;
        match actor.user_type {
            UserType::Team => {
                let owns = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (SELECT 1 FROM team_accounts WHERE user_id = $1 AND team_id = $2)",
                )
                .bind(actor.id)
                .bind(team_id)
                .fetch_one(&self.database)
                .await
                .map_err(|error| AppError::internal("check team account ownership", error))?;
                if !owns {
                    return Err(team_not_found());
                }
            }
            UserType::Staff
                if actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
                    && !actor.is_super_admin() =>
            {
                let managed = sqlx::query_scalar::<_, bool>(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN contests contest
                          ON contest.id = ct.contest_id AND contest.deleted_at IS NULL
                        JOIN contest_management_assignments caa
                          ON caa.contest_id = ct.contest_id
                        WHERE ct.team_id = $1 AND caa.user_id = $2
                    )
                    "#,
                )
                .bind(team_id)
                .bind(actor.id)
                .fetch_one(&self.database)
                .await
                .map_err(|error| AppError::internal("check visible team scope", error))?;
                if !managed {
                    return Err(team_not_found());
                }
            }
            UserType::SuperAdmin => {}
            _ => return Err(team_not_found()),
        }
        self.load(team_id, false).await
    }

    pub async fn update(
        &self,
        team_id: i64,
        request: ValidatedUpdateTeam,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<TeamResponse, AppError> {
        require_positive_team_id(team_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team update", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let current = sqlx::query_as::<
            _,
            (String, Option<String>, Option<String>, Option<String>, bool, i64),
        >(
            r#"
            SELECT name, school, seat_no, group_name, star, version
            FROM teams
            WHERE id = $1 AND deleted_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock team", error))?
        .ok_or_else(team_not_found)?;
        if request.expected_version.is_some_and(|version| version != current.5) {
            return Err(AppError::conflict(
                "TEAM_VERSION_CONFLICT",
                "Team was modified by another request",
            ));
        }
        let name = request.name.unwrap_or(current.0);
        let school = request.school.or(current.1);
        let seat_no = request.seat_no.or(current.2);
        let group_name = request.group_name.or(current.3);
        let star = request.star.unwrap_or(current.4);
        sqlx::query(
            r#"
            UPDATE teams
            SET name = $1,
                school = $2,
                seat_no = $3,
                group_name = $4,
                star = $5,
                version = version + 1,
                updated_at = now()
            WHERE id = $6
            "#,
        )
        .bind(&name)
        .bind(school)
        .bind(seat_no)
        .bind(group_name)
        .bind(star)
        .bind(team_id)
        .execute(&mut *transaction)
        .await
        .map_err(map_team_write_error)?;
        sqlx::query(
            r#"
            UPDATE users
            SET display_name = $1, updated_at = now()
            WHERE id = (SELECT user_id FROM team_accounts WHERE team_id = $2)
            "#,
        )
        .bind(name)
        .bind(team_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("synchronize team account display name", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_UPDATED",
            "team",
            &team_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        enqueue_for_team_contests(
            &mut transaction,
            team_id,
            "TEAM_UPDATED",
            serde_json::json!({"teamId": team_id}),
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team update", error))?;
        self.load(team_id, false).await
    }

    pub async fn delete(
        &self,
        team_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        require_positive_team_id(team_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team deletion", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        sqlx::query("SELECT id FROM teams WHERE id = $1 AND deleted_at IS NULL FOR UPDATE")
            .bind(team_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock team for deletion", error))?
            .ok_or_else(team_not_found)?;
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM contest_teams WHERE team_id = $1)",
        )
        .bind(team_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("check team contest assignments", error))?;
        if assigned {
            return Err(AppError::conflict(
                "TEAM_IN_USE",
                "Team must be removed from every contest before deletion",
            ));
        }
        let user_id = sqlx::query_scalar::<_, i64>(
            "SELECT user_id FROM team_accounts WHERE team_id = $1 FOR UPDATE",
        )
        .bind(team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock team account", error))?;
        sqlx::query(
            "UPDATE teams SET deleted_at = now(), updated_at = now(), version = version + 1 WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(team_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("soft delete team", error))?;
        if let Some(user_id) = user_id {
            sqlx::query("UPDATE users SET enabled = false, updated_at = now() WHERE id = $1")
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("disable deleted team account", error))?;
            sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("revoke deleted team sessions", error))?;
        }
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_DELETED",
            "team",
            &team_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team deletion", error))
    }

    async fn load(&self, team_id: i64, include_deleted: bool) -> Result<TeamResponse, AppError> {
        let sql = format!(
            r#"
            SELECT {TEAM_COLUMNS}
            FROM teams t
            LEFT JOIN team_accounts account ON account.team_id = t.id
            LEFT JOIN users account_user ON account_user.id = account.user_id
            WHERE t.id = $1 AND ($2 OR t.deleted_at IS NULL)
            "#
        );
        sqlx::query_as::<_, TeamRow>(sqlx::AssertSqlSafe(sql))
            .bind(team_id)
            .bind(include_deleted)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load team", error))?
            .ok_or_else(team_not_found)?
            .response()
    }
}
