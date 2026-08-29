mod helpers;

#[cfg(test)]
mod tests;

use std::{collections::HashSet, net::IpAddr};

use sqlx::PgPool;

use crate::{error::AppError, features::auth::model::AuthUser};

use crate::features::contest_problems::model::{
    ContestProblemDetailResponse, ContestProblemDetailRow, ContestProblemResponse,
    ValidatedAssignment, ValidatedAssignmentUpdate, ValidatedReorderEntry,
};
use helpers::{
    assignment_not_found, lock_configurable_contest, map_assignment_write_error, record_audit,
    record_reorder_audit, require_ids, require_positive_contest_id, require_readable,
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
