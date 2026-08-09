use std::net::IpAddr;

use axum::body::Body;
use bytes::Bytes;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    object_storage::{ObjectStorageHandle, keys},
    object_storage_cleanup::{
        attempt_queued_cleanup, defer_failed_cleanup, enqueue_cleanup_transaction,
    },
    pagination::PageResponse,
};

use super::markdown::render_safe;
use super::model::{
    AttachmentKind, ProblemAttachmentResponse, ProblemListQuery, ProblemResponse, ProblemRow,
    ProblemStatementResponse, ProblemStatementRow, ProblemTestdataResponse,
    ProblemTestdataVersionResponse, ValidatedProblem, ValidatedProblemUpdate, ValidatedStatement,
    validate_languages_for_judge_mode,
};
use super::testdata_archive;

const PROBLEM_COLUMNS: &str = r#"
    id, slug, title, time_limit_ms, memory_limit_mb, output_limit_kb,
    languages, testdata_version, testdata_sha256, default_lang_code,
    created_by, created_at, updated_at, version, judge_mode,
    interactor_object_key, interactor_sha256
"#;

pub struct ProblemService {
    database: PgPool,
}

pub struct AttachmentDownload {
    pub filename: String,
    pub content_type: Option<String>,
    pub content: Bytes,
}

/// Authorized attachment metadata used by the HTTP streaming path. The object
/// key never crosses the handler boundary into a response header or JSON body.
pub struct AttachmentDownloadReference {
    pub filename: String,
    pub content_type: Option<String>,
    pub object_key: String,
}

pub struct TestdataDownload {
    pub filename: String,
    pub content: Body,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TestdataReference {
    pub version: i32,
    pub object_key: String,
    pub sha256: String,
    pub case_count: Option<i32>,
}

impl ProblemService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list(
        &self,
        query: ProblemListQuery,
        actor: &AuthUser,
    ) -> Result<PageResponse<ProblemResponse>, AppError> {
        query.validate()?;
        require_problem_catalog_access(&self.database, query.contest_id, actor).await?;
        let offset = query.offset()?;
        let total_elements =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM problems WHERE deleted_at IS NULL")
                .fetch_one(&self.database)
                .await
                .map_err(|error| AppError::internal("count active problems", error))?;
        let sql = format!(
            r#"
            SELECT {PROBLEM_COLUMNS}
            FROM problems
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT $1 OFFSET $2
            "#
        );
        let rows = sqlx::query_as::<_, ProblemRow>(sqlx::AssertSqlSafe(sql))
            .bind(i64::from(query.size))
            .bind(offset)
            .fetch_all(&self.database)
            .await
            .map_err(|error| AppError::internal("list active problems", error))?;
        let content = rows.into_iter().map(ProblemRow::response).collect::<Result<Vec<_>, _>>()?;
        Ok(PageResponse::new(content, query.page, query.size, total_elements))
    }

    pub async fn get(
        &self,
        problem_id: i64,
        actor: &AuthUser,
    ) -> Result<ProblemResponse, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        let sql =
            format!("SELECT {PROBLEM_COLUMNS} FROM problems WHERE id = $1 AND deleted_at IS NULL");
        sqlx::query_as::<_, ProblemRow>(sqlx::AssertSqlSafe(sql))
            .bind(problem_id)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load active problem", error))?
            .ok_or_else(problem_not_found)?
            .response()
    }

    pub async fn create(
        &self,
        request: ValidatedProblem,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<ProblemResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem creation", error))?;
        let sql = format!(
            r#"
            INSERT INTO problems
                (slug, title, time_limit_ms, memory_limit_mb, output_limit_kb,
                 languages, default_lang_code, judge_mode, interactor_object_key,
                 interactor_sha256, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING {PROBLEM_COLUMNS}
            "#
        );
        let row = sqlx::query_as::<_, ProblemRow>(sqlx::AssertSqlSafe(sql))
            .bind(request.slug)
            .bind(request.title)
            .bind(request.time_limit_ms)
            .bind(request.memory_limit_mb)
            .bind(request.output_limit_kb)
            .bind(request.languages_json)
            .bind(request.default_lang_code)
            .bind(request.judge_mode)
            .bind(request.interactor_object_key)
            .bind(request.interactor_sha256)
            .bind(actor_user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_write_error)?;
        record_audit(&mut transaction, actor_user_id, "PROBLEM_CREATED", row.id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem creation", error))?;
        row.response()
    }

    pub async fn update(
        &self,
        problem_id: i64,
        request: ValidatedProblemUpdate,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemResponse, AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem update", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let sql = format!(
            r#"
            UPDATE problems
            SET slug = COALESCE($1, slug),
                title = COALESCE($2, title),
                time_limit_ms = COALESCE($3, time_limit_ms),
                memory_limit_mb = COALESCE($4, memory_limit_mb),
                output_limit_kb = COALESCE($5, output_limit_kb),
                languages = COALESCE($6, languages),
                default_lang_code = COALESCE($7, default_lang_code),
                judge_mode = COALESCE($8, judge_mode),
                interactor_object_key = CASE WHEN $8 IS NULL THEN interactor_object_key WHEN $8 = 'INTERACTIVE' THEN COALESCE($9, interactor_object_key) ELSE NULL END,
                interactor_sha256 = CASE WHEN $8 IS NULL THEN interactor_sha256 WHEN $8 = 'INTERACTIVE' THEN COALESCE($10, interactor_sha256) ELSE NULL END,
                updated_at = now(),
                version = version + 1
            WHERE id = $11 AND deleted_at IS NULL AND version = $12
            RETURNING {PROBLEM_COLUMNS}
            "#
        );
        let row = sqlx::query_as::<_, ProblemRow>(sqlx::AssertSqlSafe(sql))
            .bind(request.slug)
            .bind(request.title)
            .bind(request.time_limit_ms)
            .bind(request.memory_limit_mb)
            .bind(request.output_limit_kb)
            .bind(request.languages_json)
            .bind(request.default_lang_code)
            .bind(request.judge_mode)
            .bind(request.interactor_object_key)
            .bind(request.interactor_sha256)
            .bind(problem_id)
            .bind(request.expected_version)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_write_error)?;
        let row = match row {
            Some(row) => row,
            None => return Err(classify_missing_or_stale(&mut transaction, problem_id).await?),
        };
        let languages: Vec<String> = serde_json::from_str(&row.languages)
            .map_err(|error| AppError::internal("decode updated problem languages", error))?;
        validate_languages_for_judge_mode(&languages, &row.judge_mode)?;
        record_audit(&mut transaction, actor.id, "PROBLEM_UPDATED", problem_id, request_ip).await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem update", error))?;
        row.response()
    }

    pub async fn delete(
        &self,
        problem_id: i64,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem deletion", error))?;
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM contest_problems WHERE problem_id = $1)",
        )
        .bind(problem_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("check problem assignments", error))?;
        if assigned {
            return Err(AppError::conflict(
                "PROBLEM_ASSIGNED_TO_CONTEST",
                "An assigned problem cannot be deleted",
            ));
        }
        let changed = sqlx::query(
            r#"
            UPDATE problems
            SET deleted_at = now(), updated_at = now(), version = version + 1
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(problem_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("soft delete problem", error))?
        .rows_affected();
        if changed == 0 {
            return Err(problem_not_found());
        }
        record_audit(&mut transaction, actor_user_id, "PROBLEM_DELETED", problem_id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem deletion", error))
    }
}
async fn require_problem_catalog_access(
    database: &PgPool,
    contest_id: Option<i64>,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let Some(contest_id) = contest_id else {
        return Err(AppError::forbidden(
            "FORBIDDEN",
            "Contest managers must provide a contest scope",
        ));
    };
    let manageable = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contests c
            JOIN contest_management_assignments scope ON scope.contest_id = c.id
            WHERE c.id = $1 AND c.deleted_at IS NULL AND scope.user_id = $2
        )
        "#,
    )
    .bind(contest_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check scoped problem catalog access", error))?;
    if manageable {
        Ok(())
    } else {
        Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))
    }
}

async fn preflight_attachment_change(
    database: &PgPool,
    problem_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM problems WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(problem_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("preflight attachment problem", error))?;
    if !exists {
        return Err(problem_not_found());
    }
    require_problem_manage_pool(database, problem_id, actor).await?;
    let frozen = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_problems cp
            JOIN contests c ON c.id = cp.contest_id
            WHERE cp.problem_id = $1 AND c.deleted_at IS NULL AND c.status <> 'DRAFT'
        )
        "#,
    )
    .bind(problem_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("preflight attachment freeze", error))?;
    if frozen {
        Err(AppError::conflict(
            "PROBLEM_CONFIG_FROZEN",
            "Problem configuration is used by a frozen or started contest",
        ))
    } else {
        Ok(())
    }
}

async fn lock_attachment_change(
    transaction: &mut Transaction<'_, Postgres>,
    problem_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    sqlx::query("SELECT id FROM problems WHERE id = $1 AND deleted_at IS NULL FOR UPDATE")
        .bind(problem_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("lock attachment problem", error))?
        .ok_or_else(problem_not_found)?;
    require_problem_manage_transaction(transaction, problem_id, actor).await?;
    let frozen = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_problems cp
            JOIN contests c ON c.id = cp.contest_id
            WHERE cp.problem_id = $1 AND c.deleted_at IS NULL AND c.status <> 'DRAFT'
        )
        "#,
    )
    .bind(problem_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("recheck attachment freeze", error))?;
    if frozen {
        Err(AppError::conflict(
            "PROBLEM_CONFIG_FROZEN",
            "Problem configuration is used by a frozen or started contest",
        ))
    } else {
        Ok(())
    }
}

async fn require_problem_manage_pool(
    database: &PgPool,
    problem_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let (total, managed) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            count(*),
            count(*) FILTER (WHERE EXISTS (
                SELECT 1 FROM contest_management_assignments caa
                WHERE caa.user_id = $2 AND caa.contest_id = cp.contest_id
            ))
        FROM contest_problems cp
        JOIN contests c ON c.id = cp.contest_id AND c.deleted_at IS NULL
        WHERE cp.problem_id = $1
        "#,
    )
    .bind(problem_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check problem management scope", error))?;
    if total > 0 && total == managed { Ok(()) } else { Err(problem_not_found()) }
}

async fn require_problem_manage_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    problem_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let (total, managed) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            count(*),
            count(*) FILTER (WHERE EXISTS (
                SELECT 1 FROM contest_management_assignments caa
                WHERE caa.user_id = $2 AND caa.contest_id = cp.contest_id
            ))
        FROM contest_problems cp
        JOIN contests c ON c.id = cp.contest_id AND c.deleted_at IS NULL
        WHERE cp.problem_id = $1
        "#,
    )
    .bind(problem_id)
    .bind(actor.id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("lock problem management scope", error))?;
    if total > 0 && total == managed { Ok(()) } else { Err(problem_not_found()) }
}

async fn require_problem_readable(
    database: &PgPool,
    problem_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM problems WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(problem_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check readable attachment problem", error))?;
    if !exists {
        return Err(problem_not_found());
    }
    if actor.user_type == UserType::Team {
        let readable = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM contest_problems cp
                JOIN contests c ON c.id = cp.contest_id AND c.deleted_at IS NULL
                JOIN contest_teams roster ON roster.contest_id = c.id
                JOIN team_accounts account ON account.team_id = roster.team_id
                WHERE cp.problem_id = $1
                  AND account.user_id = $2
                  AND c.status IN ('RUNNING', 'PAUSED', 'ENDED', 'ARCHIVED')
            )
            "#,
        )
        .bind(problem_id)
        .bind(actor.id)
        .fetch_one(database)
        .await
        .map_err(|error| AppError::internal("check team attachment access", error))?;
        return if readable { Ok(()) } else { Err(attachment_not_found()) };
    }
    if actor.is_super_admin()
        || actor.has_permission(crate::features::auth::permissions::CLARIFICATION_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::BALLOON_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::RESOLVER_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::AWARD_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::SCREEN_MANAGE)
        || actor.has_permission(crate::features::auth::permissions::LIVE_MANAGE)
    {
        return Ok(());
    }
    let readable = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_problems cp
            JOIN contests c ON c.id = cp.contest_id AND c.deleted_at IS NULL
            JOIN contest_management_assignments caa ON caa.contest_id = cp.contest_id
            WHERE cp.problem_id = $1 AND caa.user_id = $2
        )
        "#,
    )
    .bind(problem_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check staff attachment access", error))?;
    if readable { Ok(()) } else { Err(attachment_not_found()) }
}

async fn classify_missing_or_stale(
    transaction: &mut Transaction<'_, Postgres>,
    problem_id: i64,
) -> Result<AppError, AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM problems WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(problem_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("classify failed problem update", error))?;
    Ok(if exists {
        AppError::conflict("PROBLEM_VERSION_STALE", "Problem was changed by another request")
    } else {
        problem_not_found()
    })
}

async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    problem_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES ($1, $2, 'PROBLEM', $3, $4, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(problem_id.to_string())
    .bind(request_ip.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record problem audit", error))
}

fn require_positive_id(problem_id: i64) -> Result<(), AppError> {
    if problem_id > 0 { Ok(()) } else { Err(AppError::validation("problemId", "must be positive")) }
}

fn problem_not_found() -> AppError {
    AppError::not_found("PROBLEM_NOT_FOUND", "Problem was not found")
}

fn attachment_not_found() -> AppError {
    AppError::not_found("ATTACHMENT_NOT_FOUND", "Problem attachment was not found")
}

fn testdata_not_found() -> AppError {
    AppError::not_found("TESTDATA_NOT_FOUND", "Problem test data was not found")
}

fn testdata_version_not_found() -> AppError {
    AppError::not_found("TESTDATA_VERSION_NOT_FOUND", "Test-data version was not found")
}

fn map_write_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("idx_problems_active_slug_unique")
    {
        AppError::conflict("PROBLEM_SLUG_TAKEN", "Problem slug is already in use")
    } else {
        AppError::internal("write problem", error)
    }
}

mod attachments;
mod statements;
mod testdata;
#[cfg(test)]
mod tests;
