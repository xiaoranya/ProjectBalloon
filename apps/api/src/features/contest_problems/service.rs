use std::{collections::HashSet, net::IpAddr};

use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
};

use super::model::{
    ContestProblemDetailResponse, ContestProblemDetailRow, ContestProblemResponse,
    ValidatedAssignment, ValidatedAssignmentUpdate, ValidatedReorderEntry,
};

const ASSIGNMENT_COLUMNS: &str = "contest_id, problem_id, alias, display_order, color, created_at";

pub struct ContestProblemService {
    database: PgPool,
}

impl ContestProblemService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list_readable(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        preferred_lang: Option<String>,
    ) -> Result<Vec<ContestProblemDetailResponse>, AppError> {
        require_positive_contest_id(contest_id)?;
        require_readable(&self.database, contest_id, actor).await?;
        let rows = sqlx::query_as::<_, ContestProblemDetailRow>(
            r#"
            SELECT
                cp.contest_id,
                cp.problem_id,
                cp.alias,
                cp.display_order,
                cp.color,
                p.slug,
                p.title,
                p.time_limit_ms,
                p.memory_limit_mb,
                p.output_limit_kb,
                p.languages,
                statement.lang_code AS statement_lang_code,
                statement.body AS statement_body,
                statement.updated_at AS statement_updated_at
            FROM contest_problems cp
            JOIN problems p ON p.id = cp.problem_id AND p.deleted_at IS NULL
            LEFT JOIN LATERAL (
                SELECT ps.lang_code, ps.body, ps.updated_at
                FROM problem_statements ps
                WHERE ps.problem_id = p.id
                ORDER BY
                    (ps.lang_code = $2) DESC NULLS LAST,
                    (ps.lang_code = p.default_lang_code) DESC,
                    ps.lang_code
                LIMIT 1
            ) statement ON true
            WHERE cp.contest_id = $1
            ORDER BY cp.display_order, cp.problem_id
            "#,
        )
        .bind(contest_id)
        .bind(preferred_lang)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list readable contest problems", error))?;
        rows.into_iter().map(ContestProblemDetailRow::response).collect()
    }

    pub async fn assign(
        &self,
        contest_id: i64,
        assignment: ValidatedAssignment,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ContestProblemResponse, AppError> {
        require_positive_contest_id(contest_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest problem assignment", error))?;
        lock_configurable_contest(&mut transaction, contest_id, actor).await?;
        let problem_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM problems WHERE id = $1 AND deleted_at IS NULL)",
        )
        .bind(assignment.problem_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("check assigned problem", error))?;
        if !problem_exists {
            return Err(AppError::not_found("PROBLEM_NOT_FOUND", "Problem was not found"));
        }
        let sql = format!(
            r#"
            INSERT INTO contest_problems
                (contest_id, problem_id, alias, display_order, color)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {ASSIGNMENT_COLUMNS}
            "#
        );
        let row = sqlx::query_as::<_, ContestProblemResponse>(sqlx::AssertSqlSafe(sql))
            .bind(contest_id)
            .bind(assignment.problem_id)
            .bind(assignment.alias)
            .bind(assignment.display_order)
            .bind(assignment.color)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_assignment_write_error)?;
        record_audit(
            &mut transaction,
            actor.id,
            "CONTEST_PROBLEM_ASSIGNED",
            contest_id,
            row.problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest problem assignment", error))?;
        Ok(row)
    }

    pub async fn update(
        &self,
        contest_id: i64,
        problem_id: i64,
        update: ValidatedAssignmentUpdate,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ContestProblemResponse, AppError> {
        require_ids(contest_id, problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest problem update", error))?;
        lock_configurable_contest(&mut transaction, contest_id, actor).await?;
        let sql = format!(
            r#"
            UPDATE contest_problems
            SET alias = COALESCE($1, alias),
                display_order = COALESCE($2, display_order),
                color = COALESCE($3, color)
            WHERE contest_id = $4 AND problem_id = $5
            RETURNING {ASSIGNMENT_COLUMNS}
            "#
        );
        let row = sqlx::query_as::<_, ContestProblemResponse>(sqlx::AssertSqlSafe(sql))
            .bind(update.alias)
            .bind(update.display_order)
            .bind(update.color)
            .bind(contest_id)
            .bind(problem_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_assignment_write_error)?
            .ok_or_else(assignment_not_found)?;
        record_audit(
            &mut transaction,
            actor.id,
            "CONTEST_PROBLEM_UPDATED",
            contest_id,
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest problem update", error))?;
        Ok(row)
    }

    pub async fn remove(
        &self,
        contest_id: i64,
        problem_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        require_ids(contest_id, problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest problem removal", error))?;
        lock_configurable_contest(&mut transaction, contest_id, actor).await?;
        let has_submissions = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM submissions WHERE contest_id = $1 AND problem_id = $2
            )
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("check contest problem submissions", error))?;
        if has_submissions {
            return Err(AppError::conflict(
                "CONTEST_PROBLEM_HAS_SUBMISSIONS",
                "A problem with submissions cannot be removed from the contest",
            ));
        }
        let changed =
            sqlx::query("DELETE FROM contest_problems WHERE contest_id = $1 AND problem_id = $2")
                .bind(contest_id)
                .bind(problem_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("remove contest problem", error))?
                .rows_affected();
        if changed == 0 {
            return Err(assignment_not_found());
        }
        record_audit(
            &mut transaction,
            actor.id,
            "CONTEST_PROBLEM_REMOVED",
            contest_id,
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest problem removal", error))
    }

    pub async fn reorder(
        &self,
        contest_id: i64,
        entries: Vec<ValidatedReorderEntry>,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<Vec<ContestProblemResponse>, AppError> {
        require_positive_contest_id(contest_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest problem reorder", error))?;
        lock_configurable_contest(&mut transaction, contest_id, actor).await?;
        let stored_ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT problem_id
            FROM contest_problems
            WHERE contest_id = $1
            ORDER BY problem_id
            FOR UPDATE
            "#,
        )
        .bind(contest_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock contest problems for reorder", error))?;
        let requested_ids = entries.iter().map(|entry| entry.problem_id).collect::<HashSet<_>>();
        let stored_ids = stored_ids.into_iter().collect::<HashSet<_>>();
        if requested_ids != stored_ids {
            return Err(AppError::conflict(
                "CONTEST_PROBLEM_REORDER_SET_MISMATCH",
                "Reorder request must contain every assigned problem exactly once",
            ));
        }

        sqlx::query("SET CONSTRAINTS contest_problems_contest_id_display_order_key DEFERRED")
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("defer contest problem order uniqueness", error))?;
        for entry in entries {
            sqlx::query(
                r#"
                UPDATE contest_problems
                SET display_order = $1
                WHERE contest_id = $2 AND problem_id = $3
                "#,
            )
            .bind(entry.display_order)
            .bind(contest_id)
            .bind(entry.problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(map_assignment_write_error)?;
        }
        record_reorder_audit(&mut transaction, actor.id, contest_id, request_ip).await?;
        let sql = format!(
            r#"
            SELECT {ASSIGNMENT_COLUMNS}
            FROM contest_problems
            WHERE contest_id = $1
            ORDER BY display_order, problem_id
            "#
        );
        let reordered = sqlx::query_as::<_, ContestProblemResponse>(sqlx::AssertSqlSafe(sql))
            .bind(contest_id)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("load reordered contest problems", error))?;
        transaction.commit().await.map_err(map_assignment_write_error)?;
        Ok(reordered)
    }
}

async fn lock_configurable_contest(
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

async fn require_readable(
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

    if actor.has_role("SUPER_ADMIN")
        || actor.has_role("JUDGE")
        || actor.has_role("PRINTER")
        || actor.has_role("BALLOON_STAFF")
        || actor.has_role("RESOLVER_OPERATOR")
        || actor.has_role("AWARD_OPERATOR")
        || actor.has_role("SCREEN_OPERATOR")
        || actor.has_role("LIVE_OPERATOR")
    {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE user_id = $1 AND contest_id = $2)",
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check contest problem read scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

async fn require_manage_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE user_id = $1 AND contest_id = $2)",
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check contest problem mutation scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

async fn record_audit(
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

async fn record_reorder_audit(
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

fn require_positive_contest_id(contest_id: i64) -> Result<(), AppError> {
    if contest_id > 0 { Ok(()) } else { Err(AppError::validation("contestId", "must be positive")) }
}

fn require_ids(contest_id: i64, problem_id: i64) -> Result<(), AppError> {
    require_positive_contest_id(contest_id)?;
    if problem_id > 0 { Ok(()) } else { Err(AppError::validation("problemId", "must be positive")) }
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

fn assignment_not_found() -> AppError {
    AppError::not_found("CONTEST_PROBLEM_NOT_FOUND", "Contest problem assignment was not found")
}

fn map_assignment_write_error(error: sqlx::Error) -> AppError {
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use super::ContestProblemService;
    use crate::features::{
        auth::model::{AuthUser, UserType},
        contest_problems::model::{
            ValidatedAssignment, ValidatedAssignmentUpdate, ValidatedReorderEntry,
        },
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn assignment_is_locked_after_configuration_freeze(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('admin', 'test-hash', 'Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert admin");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Test', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('sum', 'Sum', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let actor = AuthUser {
            id: user_id,
            username: "admin".into(),
            display_name: "Admin".into(),
            user_type: UserType::SuperAdmin,
            roles: vec!["SUPER_ADMIN".into()],
            password_reset_required: false,
        };
        let service = ContestProblemService::new(pool.clone());
        service
            .assign(
                contest_id,
                ValidatedAssignment {
                    problem_id,
                    alias: "A".into(),
                    display_order: 1,
                    color: Some("red".into()),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("draft assignment must succeed");
        let second_problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('difference', 'Difference', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert second problem");
        service
            .assign(
                contest_id,
                ValidatedAssignment {
                    problem_id: second_problem_id,
                    alias: "B".into(),
                    display_order: 2,
                    color: Some("blue".into()),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("second draft assignment must succeed");
        let reordered = service
            .reorder(
                contest_id,
                vec![
                    ValidatedReorderEntry { problem_id, display_order: 2 },
                    ValidatedReorderEntry { problem_id: second_problem_id, display_order: 1 },
                ],
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("position exchange must succeed atomically");
        assert_eq!(reordered[0].problem_id, second_problem_id);
        assert_eq!(reordered[1].problem_id, problem_id);

        let incomplete = service
            .reorder(
                contest_id,
                vec![ValidatedReorderEntry { problem_id, display_order: 1 }],
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await;
        assert!(incomplete.is_err());
        let stored_order = sqlx::query_scalar::<_, i32>(
            "SELECT display_order FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("read order after rejected request");
        assert_eq!(stored_order, 2);

        sqlx::query("UPDATE contests SET status = 'FROZEN_CONFIG' WHERE id = $1")
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("freeze contest");
        let frozen_update = service
            .update(
                contest_id,
                problem_id,
                ValidatedAssignmentUpdate {
                    alias: Some("B".into()),
                    display_order: None,
                    color: None,
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await;
        assert!(frozen_update.is_err());
        let frozen_reorder = service
            .reorder(
                contest_id,
                vec![
                    ValidatedReorderEntry { problem_id, display_order: 1 },
                    ValidatedReorderEntry { problem_id: second_problem_id, display_order: 2 },
                ],
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await;
        assert!(frozen_reorder.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn contest_admin_can_assign_and_remove_within_scope(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('problem-manager', 'test-hash', 'Problem Manager', 'CONTEST_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest admin");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Scoped Assignment', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('scoped-assignment', 'Scoped Assignment', $1) RETURNING id",
        )
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query("INSERT INTO contest_admin_assignments (user_id, contest_id) VALUES ($1, $2)")
            .bind(admin_id)
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("assign contest admin scope");
        let actor = AuthUser {
            id: admin_id,
            username: "problem-manager".into(),
            display_name: "Problem Manager".into(),
            user_type: UserType::ContestAdmin,
            roles: vec!["CONTEST_ADMIN".into()],
            password_reset_required: false,
        };
        let service = ContestProblemService::new(pool.clone());
        service
            .assign(
                contest_id,
                ValidatedAssignment {
                    problem_id,
                    alias: "A".into(),
                    display_order: 1,
                    color: Some("red".into()),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("scoped contest admin can assign problem");
        service
            .remove(contest_id, problem_id, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("scoped contest admin can remove problem");
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("count removed assignment");
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn rostered_team_sees_only_started_contest_with_safe_preferred_statement(pool: PgPool) {
        let team_user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('team-1', 'test-hash', 'Team 1', 'TEAM', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert team user");
        let team_id =
            sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Team 1') RETURNING id")
                .fetch_one(&pool)
                .await
                .expect("insert team");
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(team_user_id)
            .bind(team_id)
            .execute(&pool)
            .await
            .expect("link team account");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Team Test', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        sqlx::query(
            "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
        )
        .bind(contest_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("insert roster");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, default_lang_code) VALUES ('sum', 'Sum', 'en') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
        sqlx::query(
            r#"
            INSERT INTO problem_statements (problem_id, lang_code, body)
            VALUES
                ($1, 'en', '# Sum'),
                ($1, 'zh-CN', '# 求和<script>alert(1)</script>')
            "#,
        )
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("insert statements");
        let actor = AuthUser {
            id: team_user_id,
            username: "team-1".into(),
            display_name: "Team 1".into(),
            user_type: UserType::Team,
            roles: vec!["TEAM_LEADER".into()],
            password_reset_required: false,
        };
        let service = ContestProblemService::new(pool.clone());
        assert!(service.list_readable(contest_id, &actor, Some("zh-CN".into())).await.is_err());

        sqlx::query("UPDATE contests SET status = 'RUNNING' WHERE id = $1")
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("start contest");
        let problems = service
            .list_readable(contest_id, &actor, Some("zh-CN".into()))
            .await
            .expect("rostered team can list started contest problems");
        assert_eq!(problems.len(), 1);
        let statement = problems[0].statement.as_ref().expect("preferred statement");
        assert_eq!(statement.lang_code, "zh-CN");
        assert!(statement.rendered_html.contains("求和"));
        assert!(!statement.rendered_html.contains("<script"));
    }
}
