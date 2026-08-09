use std::net::IpAddr;

use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{
        hash_password,
        model::{AuthUser, UserType},
    },
    pagination::PageResponse,
};

use super::model::{
    BatchImportResponse, BatchImportRowResponse, ContestTeamResponse, ParticipationType,
    TeamMemberResponse, TeamResponse, TeamRow, ValidatedBatchImport,
    ValidatedContestTeamAssignment, ValidatedCreateTeam, ValidatedTeamListQuery,
    ValidatedTeamMember, ValidatedTeamMemberPatch, ValidatedUpdateTeam,
};

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
    database: PgPool,
}

struct PreparedTeam {
    request: ValidatedCreateTeam,
    password_hash: Option<String>,
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

    pub async fn reset_password(
        &self,
        team_id: i64,
        new_password: String,
        require_password_reset: bool,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        let hash = hash_password(new_password)
            .await
            .map_err(|error| AppError::internal("hash team password", error))?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team password reset", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let user_id = sqlx::query_scalar::<_, i64>(
            "SELECT user_id FROM team_accounts WHERE team_id = $1 FOR UPDATE",
        )
        .bind(team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock team account", error))?
        .ok_or_else(|| {
            AppError::not_found("TEAM_ACCOUNT_NOT_FOUND", "Team account was not found")
        })?;
        sqlx::query(
            "UPDATE users SET password_hash = $1, password_reset_required = $2, updated_at = now() WHERE id = $3",
        )
        .bind(hash)
        .bind(require_password_reset)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("reset team password", error))?;
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("revoke team sessions", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_PASSWORD_RESET",
            "user",
            &user_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team password reset", error))
    }

    pub async fn batch_import(
        &self,
        request: ValidatedBatchImport,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<BatchImportResponse, AppError> {
        if !actor.is_super_admin()
            && actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
            && request.contest_id.is_none()
        {
            return Err(AppError::bad_request(
                "CONTEST_REQUIRED",
                "Contest managers must import into a managed contest",
            ));
        }
        let mut prepared = Vec::with_capacity(request.teams.len());
        for team in request.teams {
            prepared.push(prepare_team(team).await?);
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team batch import", error))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&request.idempotency_key)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock team import idempotency key", error))?;
        if let Some((request_data, response_data)) =
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT request_data, response_data FROM team_import_batches WHERE idempotency_key = $1",
            )
            .bind(&request.idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("load existing team import", error))?
        {
            if request_data != request.request_hash {
                return Err(AppError::conflict(
                    "TEAM_IMPORT_IDEMPOTENCY_CONFLICT",
                    "Idempotency key was already used for another request",
                ));
            }
            let response_data = response_data.ok_or_else(|| {
                AppError::conflict("TEAM_IMPORT_IN_PROGRESS", "Team import is still in progress")
            })?;
            return serde_json::from_str(&response_data)
                .map_err(|error| AppError::internal("deserialize team import response", error));
        }
        if let Some(contest_id) = request.contest_id {
            require_manage_contest(&mut transaction, contest_id, actor).await?;
            lock_open_contest(&mut transaction, contest_id).await?;
        } else if !actor.is_super_admin() {
            return Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"));
        }
        let batch_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO team_import_batches
                (batch_id, idempotency_key, request_data, created_by_user_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(&batch_id)
        .bind(&request.idempotency_key)
        .bind(&request.request_hash)
        .bind(actor.id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("create team import batch", error))?;
        let mut created = Vec::with_capacity(prepared.len());
        for (index, prepared_team) in prepared.into_iter().enumerate() {
            let star = prepared_team.request.star;
            let (team_id, account) =
                create_team_in_transaction(&mut transaction, prepared_team).await?;
            if let Some(contest_id) = request.contest_id {
                let participation =
                    if star { ParticipationType::Star } else { request.participation_type };
                sqlx::query(
                    r#"
                    INSERT INTO contest_teams
                        (contest_id, team_id, participation_type, group_name)
                    SELECT $1, $2, $3, group_name FROM teams WHERE id = $2
                    "#,
                )
                .bind(contest_id)
                .bind(team_id)
                .bind(participation.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(map_contest_team_write_error)?;
                enqueue_realtime(
                    &mut transaction,
                    contest_id,
                    "CONTEST_TEAMS_CHANGED",
                    "TEAM",
                    Some(team_id),
                    serde_json::json!({
                        "contestId": contest_id,
                        "batchId": batch_id,
                        "action": "CONTEST_TEAM_ASSIGNED"
                    }),
                )
                .await?;
            }
            created.push(BatchImportRowResponse {
                index,
                team_id,
                user_id: account.as_ref().map(|(id, _)| *id),
                username: account.map(|(_, username)| username),
            });
        }
        let response = BatchImportResponse {
            batch_id: batch_id.clone(),
            total_requested: created.len(),
            created,
        };
        let response_json = serde_json::to_string(&response)
            .map_err(|error| AppError::internal("serialize team import response", error))?;
        sqlx::query(
            r#"
            UPDATE team_import_batches
            SET response_data = $1, completed_at = now()
            WHERE batch_id = $2
            "#,
        )
        .bind(response_json)
        .bind(&batch_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("complete team import batch", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_BATCH_IMPORTED",
            "team_import_batch",
            &batch_id,
            request_ip,
            &format!("created:{}", response.total_requested),
        )
        .await?;
        if let Some(contest_id) = request.contest_id {
            enqueue_realtime(
                &mut transaction,
                contest_id,
                "CONTEST_TEAMS_CHANGED",
                "STAFF",
                None,
                serde_json::json!({"contestId": contest_id, "batchId": batch_id}),
            )
            .await?;
        }
        transaction.commit().await.map_err(map_batch_import_error)?;
        Ok(response)
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

async fn prepare_team(request: ValidatedCreateTeam) -> Result<PreparedTeam, AppError> {
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

async fn create_team_in_transaction(
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

async fn require_manage_team(
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

async fn require_manage_contest(
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

async fn lock_open_contest(
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

async fn enqueue_for_team_contests(
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

async fn enqueue_realtime(
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

async fn record_audit(
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

fn member_id(member: &TeamMemberResponse) -> i64 {
    member.id
}

fn require_positive_team_id(team_id: i64) -> Result<(), AppError> {
    if team_id > 0 { Ok(()) } else { Err(team_not_found()) }
}

fn team_not_found() -> AppError {
    AppError::not_found("TEAM_NOT_FOUND", "Team was not found")
}

fn team_member_not_found() -> AppError {
    AppError::not_found("TEAM_MEMBER_NOT_FOUND", "Team member was not found")
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

fn map_team_write_error(error: sqlx::Error) -> AppError {
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

fn map_contest_team_write_error(error: sqlx::Error) -> AppError {
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

fn map_batch_import_error(error: sqlx::Error) -> AppError {
    match error.as_database_error().and_then(sqlx::error::DatabaseError::constraint) {
        Some("idx_teams_active_name_unique") => AppError::conflict(
            "TEAM_IMPORT_DUPLICATE_NAME",
            "Team import contains a duplicate name",
        ),
        Some("users_username_key") => AppError::conflict(
            "TEAM_IMPORT_DUPLICATE_USERNAME",
            "Team import contains a duplicate username",
        ),
        _ => AppError::internal("commit team batch import", error),
    }
}
