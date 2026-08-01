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
        let rows = sqlx::query_as::<_, ProblemRow>(&sql)
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
        sqlx::query_as::<_, ProblemRow>(&sql)
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
        let row = sqlx::query_as::<_, ProblemRow>(&sql)
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
        let row = sqlx::query_as::<_, ProblemRow>(&sql)
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

    pub async fn upsert_statement(
        &self,
        problem_id: i64,
        statement: ValidatedStatement,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemStatementResponse, AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem statement update", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let row = sqlx::query_as::<_, ProblemStatementRow>(
            r#"
            INSERT INTO problem_statements (problem_id, lang_code, body, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (problem_id, lang_code)
            DO UPDATE SET body = EXCLUDED.body, updated_at = now()
            RETURNING problem_id, lang_code, body, updated_at
            "#,
        )
        .bind(problem_id)
        .bind(statement.lang_code)
        .bind(statement.body)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("upsert problem statement", error))?;
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch statement problem", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_STATEMENT_UPSERTED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem statement update", error))?;
        Ok(ProblemStatementResponse {
            problem_id: row.problem_id,
            lang_code: row.lang_code,
            rendered_html: render_safe(&row.body),
            body: row.body,
            updated_at: row.updated_at,
        })
    }

    pub async fn list_statements(
        &self,
        problem_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ProblemStatementResponse>, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        let rows = sqlx::query_as::<_, ProblemStatementRow>(
            r#"
            SELECT problem_id, lang_code, body, updated_at
            FROM problem_statements
            WHERE problem_id = $1
            ORDER BY lang_code
            "#,
        )
        .bind(problem_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list problem statements", error))?;
        Ok(rows
            .into_iter()
            .map(|row| ProblemStatementResponse {
                problem_id: row.problem_id,
                lang_code: row.lang_code,
                rendered_html: render_safe(&row.body),
                body: row.body,
                updated_at: row.updated_at,
            })
            .collect())
    }

    pub async fn delete_statement(
        &self,
        problem_id: i64,
        lang_code: String,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        require_positive_id(problem_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin problem statement deletion", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let deleted =
            sqlx::query("DELETE FROM problem_statements WHERE problem_id = $1 AND lang_code = $2")
                .bind(problem_id)
                .bind(lang_code)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("delete problem statement", error))?
                .rows_affected();
        if deleted == 0 {
            return Err(AppError::not_found(
                "STATEMENT_NOT_FOUND",
                "Problem statement was not found",
            ));
        }
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch problem after statement deletion", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_STATEMENT_DELETED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit problem statement deletion", error))
    }

    // Upload handlers pass independently validated request, actor, and storage metadata.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_attachment(
        &self,
        problem_id: i64,
        kind: AttachmentKind,
        filename: String,
        content_type: Option<String>,
        content: Bytes,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<ProblemAttachmentResponse, AppError> {
        require_positive_id(problem_id)?;
        if content.is_empty() || content.len() > 20 * 1024 * 1024 {
            return Err(AppError::validation("file", "must contain between 1 byte and 20 MiB"));
        }
        preflight_attachment_change(&self.database, problem_id, actor).await?;

        let sha256 = hex::encode(Sha256::digest(&content));
        let object_key = keys::problem_attachment(problem_id, &sha256, &filename);
        storage
            .backend()
            .put(storage.problem_bucket(), &object_key, content_type.as_deref(), content.clone())
            .await
            .map_err(|error| AppError::internal("upload problem attachment", error))?;

        let persisted = self
            .persist_attachment(
                problem_id,
                kind,
                filename,
                content_type,
                i64::try_from(content.len())
                    .map_err(|error| AppError::internal("convert attachment size", error))?,
                sha256,
                object_key.clone(),
                actor,
                request_ip,
            )
            .await;
        if persisted.is_err()
            && let Err(cleanup_error) =
                storage.backend().delete(storage.problem_bucket(), &object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.problem_bucket(),
                &object_key,
                "PROBLEM_ATTACHMENT_UPLOAD_COMPENSATION",
                cleanup_error.to_string(),
            )
            .await;
        }
        persisted
    }

    pub async fn upload_interactor(
        &self,
        problem_id: i64,
        content: Bytes,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<ProblemResponse, AppError> {
        require_positive_id(problem_id)?;
        if content.len() < 4 || content.len() > 20 * 1024 * 1024 || &content[..4] != b"\x7fELF" {
            return Err(AppError::validation(
                "file",
                "must be a Linux ELF executable of at most 20 MiB",
            ));
        }
        preflight_attachment_change(&self.database, problem_id, actor).await?;
        let sha256 = hex::encode(Sha256::digest(&content));
        let object_key = keys::interactor(problem_id);
        storage
            .backend()
            .put(storage.problem_bucket(), &object_key, Some("application/x-executable"), content)
            .await
            .map_err(|error| AppError::internal("upload problem interactor", error))?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin interactor update", error))?;
        let persisted = async {
            lock_attachment_change(&mut transaction, problem_id, actor).await?;
            let old_key = sqlx::query_scalar::<_, Option<String>>("SELECT interactor_object_key FROM problems WHERE id=$1")
                .bind(problem_id).fetch_one(&mut *transaction).await
                .map_err(|error| AppError::internal("load previous interactor", error))?;
            sqlx::query("UPDATE problems SET judge_mode='INTERACTIVE',interactor_object_key=$2,interactor_sha256=$3,updated_at=now(),version=version+1 WHERE id=$1")
                .bind(problem_id).bind(&object_key).bind(&sha256).execute(&mut *transaction).await
                .map_err(|error| AppError::internal("persist problem interactor", error))?;
            if let Some(old_key) = old_key.filter(|key| key != &object_key) {
                enqueue_cleanup_transaction(&mut transaction, storage.problem_bucket(), &old_key, "PROBLEM_INTERACTOR_REPLACED").await
                    .map_err(|error| AppError::internal("queue previous interactor cleanup", error))?;
            }
            record_audit(&mut transaction, actor.id, "PROBLEM_INTERACTOR_UPLOADED", problem_id, request_ip).await?;
            transaction.commit().await.map_err(|error| AppError::internal("commit interactor update", error))?;
            let sql = format!("SELECT {PROBLEM_COLUMNS} FROM problems WHERE id=$1");
            sqlx::query_as::<_, ProblemRow>(&sql).bind(problem_id).fetch_one(&self.database).await
                .map_err(|error| AppError::internal("load updated interactor problem", error))?.response()
        }.await;
        if persisted.is_err()
            && let Err(cleanup_error) =
                storage.backend().delete(storage.problem_bucket(), &object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.problem_bucket(),
                &object_key,
                "PROBLEM_INTERACTOR_UPLOAD_COMPENSATION",
                cleanup_error.to_string(),
            )
            .await;
        }
        persisted
    }

    pub async fn list_attachments(
        &self,
        problem_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ProblemAttachmentResponse>, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        sqlx::query_as::<_, ProblemAttachmentResponse>(
            r#"
            SELECT id, problem_id, kind, original_filename, content_type, bytes, sha256, created_at
            FROM problem_attachments
            WHERE problem_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(problem_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list problem attachments", error))
    }

    pub async fn download_attachment(
        &self,
        problem_id: i64,
        attachment_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<AttachmentDownload, AppError> {
        let reference =
            self.download_attachment_reference(problem_id, attachment_id, actor).await?;
        let content = storage
            .backend()
            .get_limited(storage.problem_bucket(), &reference.object_key, 20 * 1024 * 1024)
            .await
            .map_err(|error| AppError::internal("download problem attachment object", error))?;
        Ok(AttachmentDownload {
            filename: reference.filename,
            content_type: reference.content_type,
            content,
        })
    }

    pub async fn download_attachment_reference(
        &self,
        problem_id: i64,
        attachment_id: i64,
        actor: &AuthUser,
    ) -> Result<AttachmentDownloadReference, AppError> {
        require_positive_id(problem_id)?;
        if attachment_id <= 0 {
            return Err(AppError::validation("attachmentId", "must be positive"));
        }
        require_problem_readable(&self.database, problem_id, actor).await?;
        let row = sqlx::query_as::<_, (String, String, Option<String>)>(
            r#"
            SELECT object_key, original_filename, content_type
            FROM problem_attachments
            WHERE id = $1 AND problem_id = $2
            "#,
        )
        .bind(attachment_id)
        .bind(problem_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load attachment download metadata", error))?
        .ok_or_else(attachment_not_found)?;
        Ok(AttachmentDownloadReference { object_key: row.0, filename: row.1, content_type: row.2 })
    }

    pub async fn delete_attachment(
        &self,
        problem_id: i64,
        attachment_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<(), AppError> {
        require_positive_id(problem_id)?;
        if attachment_id <= 0 {
            return Err(AppError::validation("attachmentId", "must be positive"));
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin attachment deletion", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let object_key = sqlx::query_scalar::<_, String>(
            r#"
            DELETE FROM problem_attachments
            WHERE id = $1 AND problem_id = $2
            RETURNING object_key
            "#,
        )
        .bind(attachment_id)
        .bind(problem_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("delete attachment metadata", error))?
        .ok_or_else(attachment_not_found)?;
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch attachment problem", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_ATTACHMENT_DELETED",
            problem_id,
            request_ip,
        )
        .await?;
        enqueue_cleanup_transaction(
            &mut transaction,
            storage.problem_bucket(),
            &object_key,
            "PROBLEM_ATTACHMENT_DELETION",
        )
        .await
        .map_err(|error| AppError::internal("queue attachment object cleanup", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit attachment deletion", error))?;
        attempt_queued_cleanup(&self.database, storage, storage.problem_bucket(), &object_key)
            .await;
        Ok(())
    }

    pub async fn upload_testdata(
        &self,
        problem_id: i64,
        content: Bytes,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<ProblemTestdataResponse, AppError> {
        require_positive_id(problem_id)?;
        if content.is_empty() || content.len() > 256 * 1024 * 1024 {
            return Err(AppError::validation("file", "must contain between 1 byte and 256 MiB"));
        }
        let archive = testdata_archive::validate(content.clone()).await?;
        preflight_attachment_change(&self.database, problem_id, actor).await?;
        let (previous_version, maximum_version) = sqlx::query_as::<_, (i32, i32)>(
            r#"
            SELECT problem.testdata_version,coalesce(max(version.version),0)::integer
            FROM problems problem
            LEFT JOIN problem_testdata_versions version ON version.problem_id=problem.id
            WHERE problem.id=$1 AND problem.deleted_at IS NULL
            GROUP BY problem.id,problem.testdata_version
            "#,
        )
        .bind(problem_id)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("load current test-data version", error))?;
        let version = maximum_version.checked_add(1).ok_or_else(|| {
            AppError::conflict("TESTDATA_VERSION_EXHAUSTED", "Test-data version is exhausted")
        })?;
        let sha256 = hex::encode(Sha256::digest(&content));
        let object_key = keys::testdata(problem_id, version);
        storage
            .backend()
            .put(storage.problem_bucket(), &object_key, Some("application/zip"), content.clone())
            .await
            .map_err(|error| AppError::internal("upload problem test data", error))?;
        let persisted = self
            .persist_testdata(
                problem_id,
                previous_version,
                version,
                archive.case_count,
                i64::try_from(content.len())
                    .map_err(|error| AppError::internal("convert test-data size", error))?,
                sha256,
                object_key.clone(),
                actor,
                request_ip,
            )
            .await;
        if persisted.is_err()
            && let Err(cleanup_error) =
                storage.backend().delete(storage.problem_bucket(), &object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.problem_bucket(),
                &object_key,
                "PROBLEM_TESTDATA_UPLOAD_COMPENSATION",
                cleanup_error.to_string(),
            )
            .await;
        }
        persisted
    }

    pub async fn download_testdata(
        &self,
        problem_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<TestdataDownload, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        let row = sqlx::query_as::<_, (i32, String, String)>(
            r#"
            SELECT testdata_version, testdata_object_key, testdata_sha256
            FROM problems
            WHERE id = $1 AND deleted_at IS NULL
              AND testdata_version > 0 AND testdata_object_key IS NOT NULL
              AND testdata_sha256 IS NOT NULL
            "#,
        )
        .bind(problem_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load current test-data metadata", error))?
        .ok_or_else(testdata_not_found)?;
        let content = testdata_download_body(storage, &row.1, &row.2).await?;
        Ok(TestdataDownload {
            filename: format!("problem-{problem_id}-testdata-v{}.zip", row.0),
            content,
        })
    }

    pub async fn list_testdata_versions(
        &self,
        problem_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ProblemTestdataVersionResponse>, AppError> {
        require_positive_id(problem_id)?;
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        sqlx::query_as::<_, ProblemTestdataVersionResponse>(
            r#"
            SELECT version.problem_id,version.version,version.case_count,version.bytes,
                   version.sha256,version.uploaded_by_user_id,
                   version.version=problem.testdata_version AS active,version.created_at
            FROM problem_testdata_versions version
            JOIN problems problem ON problem.id=version.problem_id AND problem.deleted_at IS NULL
            WHERE version.problem_id=$1
            ORDER BY version.version DESC
            LIMIT 1000
            "#,
        )
        .bind(problem_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list test-data versions", error))
    }

    pub async fn download_testdata_version(
        &self,
        problem_id: i64,
        version: i32,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<TestdataDownload, AppError> {
        require_positive_id(problem_id)?;
        if version <= 0 {
            return Err(testdata_version_not_found());
        }
        require_problem_manage_pool(&self.database, problem_id, actor).await?;
        let row = sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT version.object_key,version.sha256
            FROM problem_testdata_versions version
            JOIN problems problem ON problem.id=version.problem_id AND problem.deleted_at IS NULL
            WHERE version.problem_id=$1 AND version.version=$2
            "#,
        )
        .bind(problem_id)
        .bind(version)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load test-data version metadata", error))?
        .ok_or_else(testdata_version_not_found)?;
        let content = testdata_download_body(storage, &row.0, &row.1).await?;
        Ok(TestdataDownload {
            filename: format!("problem-{problem_id}-testdata-v{version}.zip"),
            content,
        })
    }

    pub async fn activate_testdata_version(
        &self,
        problem_id: i64,
        version: i32,
        expected_current_version: i32,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemTestdataVersionResponse, AppError> {
        require_positive_id(problem_id)?;
        if version <= 0 {
            return Err(testdata_version_not_found());
        }
        if expected_current_version < 0 {
            return Err(AppError::validation("expectedCurrentVersion", "must not be negative"));
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin test-data version activation", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let current_version =
            sqlx::query_scalar::<_, i32>("SELECT testdata_version FROM problems WHERE id=$1")
                .bind(problem_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("load active test-data version", error))?;
        if current_version != expected_current_version {
            return Err(AppError::conflict(
                "TESTDATA_VERSION_STALE",
                "Test data was changed by another request",
            ));
        }
        let target = sqlx::query_as::<_, (String, String)>(
            "SELECT object_key,sha256 FROM problem_testdata_versions WHERE problem_id=$1 AND version=$2",
        )
        .bind(problem_id)
        .bind(version)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("load test-data activation target", error))?
        .ok_or_else(testdata_version_not_found)?;
        if current_version != version {
            sqlx::query("UPDATE problems SET testdata_version=$2,testdata_object_key=$3,testdata_sha256=$4,updated_at=now(),version=version+1 WHERE id=$1")
                .bind(problem_id).bind(version).bind(target.0).bind(target.1)
                .execute(&mut *transaction).await
                .map_err(|error| AppError::internal("activate test-data version", error))?;
            record_audit(
                &mut transaction,
                actor.id,
                "PROBLEM_TESTDATA_VERSION_ACTIVATED",
                problem_id,
                request_ip,
            )
            .await?;
        }
        let response = load_testdata_version(&mut transaction, problem_id, version).await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit test-data version activation", error))?;
        Ok(response)
    }

    pub async fn current_testdata_reference(
        &self,
        problem_id: i64,
    ) -> Result<TestdataReference, AppError> {
        require_positive_id(problem_id)?;
        let reference = sqlx::query_as::<_, (i32, String, String, Option<i32>)>(
            r#"
            SELECT version.version, version.object_key, version.sha256, version.case_count
            FROM problems problem
            JOIN problem_testdata_versions version
              ON version.problem_id = problem.id
             AND version.version = problem.testdata_version
             AND version.object_key = problem.testdata_object_key
             AND version.sha256 = problem.testdata_sha256
            WHERE problem.id = $1 AND problem.deleted_at IS NULL
            "#,
        )
        .bind(problem_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load authoritative test-data reference", error))?
        .ok_or_else(|| {
            AppError::conflict(
                "TESTDATA_REFERENCE_INCONSISTENT",
                "Problem has no consistent current test-data version",
            )
        })?;
        Ok(TestdataReference {
            version: reference.0,
            object_key: reference.1,
            sha256: reference.2,
            case_count: reference.3,
        })
    }

    // Upload handlers pass independently validated request, actor, and storage metadata.
    #[allow(clippy::too_many_arguments)]
    async fn persist_testdata(
        &self,
        problem_id: i64,
        previous_version: i32,
        version: i32,
        case_count: i32,
        bytes: i64,
        sha256: String,
        object_key: String,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemTestdataResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin test-data metadata write", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let current_version =
            sqlx::query_scalar::<_, i32>("SELECT testdata_version FROM problems WHERE id = $1")
                .bind(problem_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("lock current test-data version", error))?;
        if current_version != previous_version {
            return Err(AppError::conflict(
                "TESTDATA_VERSION_STALE",
                "Test data was changed by another request",
            ));
        }
        let response = sqlx::query_as::<_, ProblemTestdataResponse>(
            r#"
            INSERT INTO problem_testdata_versions
                (problem_id, version, object_key, sha256, bytes, case_count, uploaded_by_user_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING problem_id, version, case_count, bytes, sha256, created_at
            "#,
        )
        .bind(problem_id)
        .bind(version)
        .bind(&object_key)
        .bind(&sha256)
        .bind(bytes)
        .bind(case_count)
        .bind(actor.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("insert immutable test-data version", error))?;
        sqlx::query(
            r#"
            UPDATE problems
            SET testdata_version = $2, testdata_object_key = $3, testdata_sha256 = $4,
                updated_at = now(), version = version + 1
            WHERE id = $1
            "#,
        )
        .bind(problem_id)
        .bind(version)
        .bind(object_key)
        .bind(sha256)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("activate test-data version", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_TESTDATA_UPLOADED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit test-data version", error))?;
        Ok(response)
    }

    // Upload handlers pass independently validated request, actor, and storage metadata.
    #[allow(clippy::too_many_arguments)]
    async fn persist_attachment(
        &self,
        problem_id: i64,
        kind: AttachmentKind,
        filename: String,
        content_type: Option<String>,
        bytes: i64,
        sha256: String,
        object_key: String,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ProblemAttachmentResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin attachment metadata write", error))?;
        lock_attachment_change(&mut transaction, problem_id, actor).await?;
        let row = sqlx::query_as::<_, ProblemAttachmentResponse>(
            r#"
            INSERT INTO problem_attachments
                (problem_id, kind, object_key, original_filename, content_type, bytes, sha256)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, problem_id, kind, original_filename, content_type, bytes, sha256, created_at
            "#,
        )
        .bind(problem_id)
        .bind(kind.as_str())
        .bind(object_key)
        .bind(filename)
        .bind(content_type)
        .bind(bytes)
        .bind(sha256)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("insert attachment metadata", error))?;
        sqlx::query("UPDATE problems SET updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(problem_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("touch attachment problem", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "PROBLEM_ATTACHMENT_UPLOADED",
            problem_id,
            request_ip,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit attachment metadata write", error))?;
        Ok(row)
    }
}

async fn require_problem_catalog_access(
    database: &PgPool,
    contest_id: Option<i64>,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let Some(contest_id) = contest_id else {
        return Err(AppError::forbidden(
            "FORBIDDEN",
            "Contest administrators must provide a contest scope",
        ));
    };
    let manageable = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contests c
            JOIN contest_admin_assignments scope ON scope.contest_id = c.id
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

async fn load_testdata_version(
    transaction: &mut Transaction<'_, Postgres>,
    problem_id: i64,
    version: i32,
) -> Result<ProblemTestdataVersionResponse, AppError> {
    sqlx::query_as::<_, ProblemTestdataVersionResponse>(
        r#"
        SELECT version.problem_id,version.version,version.case_count,version.bytes,
               version.sha256,version.uploaded_by_user_id,
               version.version=problem.testdata_version AS active,version.created_at
        FROM problem_testdata_versions version
        JOIN problems problem ON problem.id=version.problem_id AND problem.deleted_at IS NULL
        WHERE version.problem_id=$1 AND version.version=$2
        "#,
    )
    .bind(problem_id)
    .bind(version)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("load test-data version", error))?
    .ok_or_else(testdata_version_not_found)
}

/// Stream a test-data object to the response body while verifying its SHA-256
/// incrementally. Buffering the whole object (up to 256 MiB) per concurrent
/// download is a memory-exhaustion surface, so bytes flow through as chunks.
/// A mismatch is detected only once the stream is exhausted, so it terminates
/// the response early (logged) rather than returning a 409 before any bytes.
async fn testdata_download_body(
    storage: &ObjectStorageHandle,
    object_key: &str,
    expected_sha256: &str,
) -> Result<Body, AppError> {
    let stream = storage
        .backend()
        .get_stream_limited(storage.problem_bucket(), object_key, 256 * 1024 * 1024)
        .await
        .map_err(|error| AppError::internal("stream problem test data", error))?;
    // The hasher, expected digest, and logging key travel through the unfold
    // state so the producer closure never moves out of its own captures.
    let verified = futures_util::stream::unfold(
        (stream, false, Sha256::new(), expected_sha256.to_owned(), object_key.to_owned()),
        |(mut inner, finished, mut hasher, expected, object_key)| async move {
            if finished {
                return None;
            }
            match inner.next().await {
                Some(Ok(chunk)) => {
                    hasher.update(&chunk);
                    Some((Ok(chunk), (inner, false, hasher, expected, object_key)))
                }
                Some(Err(error)) => Some((
                    Err::<Bytes, Box<dyn std::error::Error + Send + Sync>>(
                        std::io::Error::other(format!("stream problem test data: {error}")).into(),
                    ),
                    (inner, true, hasher, expected, object_key),
                )),
                None => {
                    if hex::encode(hasher.finalize_reset()) != expected {
                        tracing::error!(
                            object_key = %object_key,
                            "test-data integrity mismatch detected while streaming download"
                        );
                        Some((
                            Err::<Bytes, Box<dyn std::error::Error + Send + Sync>>(
                                std::io::Error::other(
                                    "stored test data does not match its immutable metadata",
                                )
                                .into(),
                            ),
                            (inner, true, hasher, expected, object_key),
                        ))
                    } else {
                        None
                    }
                }
            }
        },
    );
    Ok(Body::from_stream(verified))
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
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let (total, managed) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            count(*),
            count(*) FILTER (WHERE EXISTS (
                SELECT 1 FROM contest_admin_assignments caa
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
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let (total, managed) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT
            count(*),
            count(*) FILTER (WHERE EXISTS (
                SELECT 1 FROM contest_admin_assignments caa
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
    if actor.has_role("SUPER_ADMIN")
        || actor.has_role("JUDGE")
        || actor.has_role("BALLOON_STAFF")
        || actor.has_role("RESOLVER_OPERATOR")
        || actor.has_role("AWARD_OPERATOR")
        || actor.has_role("SCREEN_OPERATOR")
        || actor.has_role("LIVE_OPERATOR")
    {
        return Ok(());
    }
    let readable = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_problems cp
            JOIN contests c ON c.id = cp.contest_id AND c.deleted_at IS NULL
            JOIN contest_admin_assignments caa ON caa.contest_id = cp.contest_id
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io::{Cursor, Write},
        net::{IpAddr, Ipv4Addr},
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use sqlx::PgPool;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::ProblemService;
    use crate::{
        features::auth::model::{AuthUser, UserType},
        features::problems::model::{
            AttachmentKind, ProblemListQuery, ValidatedProblem, ValidatedProblemUpdate,
            ValidatedStatement,
        },
        object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
        object_storage_cleanup::{ObjectStorageCleanupConfig, ObjectStorageCleanupRunner},
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn contest_admin_catalog_requires_an_assigned_contest_scope(pool: PgPool) {
        let creator_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-owner', 'test-hash', 'Owner', 'SUPER_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert catalog owner");
        let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-admin', 'test-hash', 'Contest Admin', 'CONTEST_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest admin");
        let managed_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Managed Catalog', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert managed contest");
        let other_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Other Catalog', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert other contest");
        sqlx::query("INSERT INTO contest_admin_assignments (user_id, contest_id) VALUES ($1, $2)")
            .bind(admin_id)
            .bind(managed_contest_id)
            .execute(&pool)
            .await
            .expect("assign contest scope");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('catalog-problem', 'Catalog Problem', $1) RETURNING id",
        )
        .bind(creator_id)
        .fetch_one(&pool)
        .await
        .expect("insert catalog problem");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(managed_contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem to managed contest");
        let actor = AuthUser {
            id: admin_id,
            username: "catalog-admin".into(),
            display_name: "Contest Admin".into(),
            user_type: UserType::ContestAdmin,
            roles: vec!["CONTEST_ADMIN".into()],
            password_reset_required: false,
        };
        let service = ProblemService::new(pool.clone());

        let page = service
            .list(
                ProblemListQuery { page: 0, size: 100, contest_id: Some(managed_contest_id) },
                &actor,
            )
            .await
            .expect("assigned contest scope can read shared problem metadata");
        assert_eq!(page.total_elements, 1);
        assert_eq!(page.content[0].slug, "catalog-problem");
        let detail = service
            .get(problem_id, &actor)
            .await
            .expect("fully scoped contest admin can read problem detail");
        let updated = service
            .update(
                problem_id,
                ValidatedProblemUpdate {
                    expected_version: detail.version,
                    slug: None,
                    title: Some("Managed Problem".into()),
                    time_limit_ms: None,
                    memory_limit_mb: None,
                    output_limit_kb: None,
                    languages_json: None,
                    default_lang_code: None,
                    judge_mode: None,
                    interactor_object_key: None,
                    interactor_sha256: None,
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("fully scoped contest admin can update problem metadata");
        assert_eq!(updated.title, "Managed Problem");
        service
            .upsert_statement(
                problem_id,
                ValidatedStatement { lang_code: "en".into(), body: "# Managed".into() },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("fully scoped contest admin can update statements");

        assert!(
            service
                .list(ProblemListQuery { page: 0, size: 100, contest_id: None }, &actor)
                .await
                .is_err()
        );
        assert!(
            service
                .list(
                    ProblemListQuery { page: 0, size: 100, contest_id: Some(other_contest_id) },
                    &actor,
                )
                .await
                .is_err()
        );
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'B', 1)",
        )
        .bind(other_contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("share problem with foreign contest");
        assert!(service.get(problem_id, &actor).await.is_err());
        assert!(
            service
                .upsert_statement(
                    problem_id,
                    ValidatedStatement { lang_code: "en".into(), body: "# Forbidden".into() },
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                )
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn super_admin_can_create_and_delete_an_unassigned_problem(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-super', 'test-hash', 'Super Admin', 'SUPER_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert super admin");
        let actor = AuthUser {
            id: user_id,
            username: "catalog-super".into(),
            display_name: "Super Admin".into(),
            user_type: UserType::SuperAdmin,
            roles: vec!["SUPER_ADMIN".into()],
            password_reset_required: false,
        };
        let service = ProblemService::new(pool.clone());
        let created = service
            .create(
                ValidatedProblem {
                    slug: "created-problem".into(),
                    title: "Created Problem".into(),
                    time_limit_ms: 1_000,
                    memory_limit_mb: 256,
                    output_limit_kb: 65_536,
                    languages_json: "[\"cpp\"]".into(),
                    default_lang_code: "en".into(),
                    judge_mode: "STANDARD".into(),
                    interactor_object_key: None,
                    interactor_sha256: None,
                },
                actor.id,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("super admin can create a problem");
        assert_eq!(created.slug, "created-problem");
        service
            .delete(created.id, actor.id, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("super admin can delete an unassigned problem");
        let deleted_at = sqlx::query_scalar::<_, Option<time::OffsetDateTime>>(
            "SELECT deleted_at FROM problems WHERE id = $1",
        )
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .expect("read soft-deleted problem");
        assert!(deleted_at.is_some());
    }

    #[derive(Default)]
    struct MemoryStorage {
        objects: Mutex<HashMap<(String, String), Bytes>>,
        fail_delete: Mutex<bool>,
    }

    #[async_trait]
    impl ObjectStorage for MemoryStorage {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }

        async fn put(
            &self,
            bucket: &str,
            key: &str,
            _content_type: Option<&str>,
            content: Bytes,
        ) -> Result<(), ObjectStorageError> {
            self.objects
                .lock()
                .expect("memory storage lock")
                .insert((bucket.to_owned(), key.to_owned()), content);
            Ok(())
        }

        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
            self.objects
                .lock()
                .expect("memory storage lock")
                .get(&(bucket.to_owned(), key.to_owned()))
                .cloned()
                .ok_or_else(|| ObjectStorageError::Request("not found".into()))
        }

        async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
            if *self.fail_delete.lock().expect("delete failure lock") {
                return Err(ObjectStorageError::Request("temporary delete failure".into()));
            }
            self.objects
                .lock()
                .expect("memory storage lock")
                .remove(&(bucket.to_owned(), key.to_owned()));
            Ok(())
        }
    }

    fn testdata_zip(case_name: &str, content: &[u8]) -> Bytes {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for extension in ["in", "out"] {
            writer
                .start_file(format!("{case_name}.{extension}"), options)
                .expect("start test-data fixture entry");
            writer.write_all(content).expect("write test-data fixture entry");
        }
        Bytes::from(writer.finish().expect("finish test-data fixture").into_inner())
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn statement_persists_markdown_and_returns_safe_html(pool: PgPool) {
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
        let response = ProblemService::new(pool)
            .upsert_statement(
                problem_id,
                ValidatedStatement {
                    lang_code: "en".into(),
                    body: "# Sum\n<script>alert(1)</script>".into(),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("upsert statement");
        assert!(response.body.contains("<script>"));
        assert!(response.rendered_html.contains("<h1>Sum</h1>"));
        assert!(!response.rendered_html.contains("<script>"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn attachment_lifecycle_keeps_database_and_object_storage_consistent(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('attachment-admin', 'test-hash', 'Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert admin");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('attachment', 'Attachment', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let actor = AuthUser {
            id: user_id,
            username: "attachment-admin".into(),
            display_name: "Admin".into(),
            user_type: UserType::SuperAdmin,
            roles: vec!["SUPER_ADMIN".into()],
            password_reset_required: false,
        };
        let memory = Arc::new(MemoryStorage::default());
        let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
        let content = Bytes::from_static(b"sample attachment");
        let service = ProblemService::new(pool.clone());
        let response = service
            .upload_attachment(
                problem_id,
                AttachmentKind::Sample,
                "sample.txt".into(),
                Some("text/plain".into()),
                content.clone(),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("upload attachment");
        assert_eq!(response.bytes, i64::try_from(content.len()).expect("small fixture"));
        assert_eq!(response.sha256.len(), 64);
        let object_key = sqlx::query_scalar::<_, String>(
            "SELECT object_key FROM problem_attachments WHERE id = $1",
        )
        .bind(response.id)
        .fetch_one(&pool)
        .await
        .expect("load attachment object key");
        let stored =
            memory.get("problems-test", &object_key).await.expect("stored object must exist");
        assert_eq!(stored, content);

        let download = service
            .download_attachment(problem_id, response.id, &actor, &storage)
            .await
            .expect("download attachment");
        assert_eq!(download.filename, "sample.txt");
        assert_eq!(download.content_type.as_deref(), Some("text/plain"));
        assert_eq!(download.content, content);

        *memory.fail_delete.lock().expect("delete failure lock") = true;
        service
            .delete_attachment(
                problem_id,
                response.id,
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("delete attachment");
        let metadata_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM problem_attachments WHERE id = $1)",
        )
        .bind(response.id)
        .fetch_one(&pool)
        .await
        .expect("check attachment metadata");
        assert!(!metadata_exists);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
                .fetch_one(&pool)
                .await
                .expect("count deferred attachment cleanup"),
            1
        );
        assert_eq!(
            memory.get("problems-test", &object_key).await.expect("orphan retained for retry"),
            content
        );
        *memory.fail_delete.lock().expect("delete failure lock") = false;
        let cleanup_runner = ObjectStorageCleanupRunner::new(
            pool.clone(),
            storage.clone(),
            ObjectStorageCleanupConfig {
                poll_interval: Duration::from_secs(1),
                lease: Duration::from_secs(30),
                retry_base: Duration::from_millis(1),
                batch_size: 10,
            },
        );
        assert_eq!(cleanup_runner.run_once().await.expect("retry attachment cleanup"), 1);
        assert!(memory.get("problems-test", &object_key).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
                .fetch_one(&pool)
                .await
                .expect("count completed attachment cleanup"),
            0
        );

        let published = service
            .upload_attachment(
                problem_id,
                AttachmentKind::Supplement,
                "guide.pdf".into(),
                Some("application/pdf".into()),
                Bytes::from_static(b"published guide"),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("upload attachment for team publication test");
        let team_user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('attachment-team', 'test-hash', 'Attachment Team', 'TEAM', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert team user");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Attachment Team') RETURNING id",
        )
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
            "INSERT INTO contests (name, status, visibility) VALUES ('Attachment Contest', 'DRAFT', 'PRIVATE') RETURNING id",
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
        .expect("insert contest roster");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
        let team_actor = AuthUser {
            id: team_user_id,
            username: "attachment-team".into(),
            display_name: "Attachment Team".into(),
            user_type: UserType::Team,
            roles: vec!["TEAM_LEADER".into()],
            password_reset_required: false,
        };
        assert!(
            service
                .download_attachment(problem_id, published.id, &team_actor, &storage)
                .await
                .is_err()
        );
        sqlx::query("UPDATE contests SET status = 'RUNNING' WHERE id = $1")
            .bind(contest_id)
            .execute(&pool)
            .await
            .expect("start contest");
        let team_download = service
            .download_attachment(problem_id, published.id, &team_actor, &storage)
            .await
            .expect("rostered team downloads started contest attachment");
        assert_eq!(team_download.content, Bytes::from_static(b"published guide"));
        assert!(
            service
                .delete_attachment(
                    problem_id,
                    published.id,
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    &storage,
                )
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn contest_admin_must_manage_every_problem_assignment_before_upload(pool: PgPool) {
        let admin_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('scoped-admin', 'test-hash', 'Scoped Admin', 'CONTEST_ADMIN', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest admin");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('shared-problem', 'Shared') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let first_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Managed Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert managed contest");
        let second_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Foreign Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert foreign contest");
        for (contest_id, alias) in [(first_contest_id, "A"), (second_contest_id, "B")] {
            sqlx::query(
                "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, $3, 1)",
            )
            .bind(contest_id)
            .bind(problem_id)
            .bind(alias)
            .execute(&pool)
            .await
            .expect("assign shared problem");
        }
        sqlx::query("INSERT INTO contest_admin_assignments (user_id, contest_id) VALUES ($1, $2)")
            .bind(admin_id)
            .bind(first_contest_id)
            .execute(&pool)
            .await
            .expect("assign first contest scope");
        let actor = AuthUser {
            id: admin_id,
            username: "scoped-admin".into(),
            display_name: "Scoped Admin".into(),
            user_type: UserType::ContestAdmin,
            roles: vec!["CONTEST_ADMIN".into()],
            password_reset_required: false,
        };
        let memory = Arc::new(MemoryStorage::default());
        let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
        let service = ProblemService::new(pool.clone());
        assert!(
            service
                .upload_attachment(
                    problem_id,
                    AttachmentKind::Sample,
                    "sample.txt".into(),
                    Some("text/plain".into()),
                    Bytes::from_static(b"sample"),
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    &storage,
                )
                .await
                .is_err()
        );
        assert!(memory.objects.lock().expect("memory storage lock").is_empty());

        sqlx::query("INSERT INTO contest_admin_assignments (user_id, contest_id) VALUES ($1, $2)")
            .bind(admin_id)
            .bind(second_contest_id)
            .execute(&pool)
            .await
            .expect("assign second contest scope");
        let attachment = service
            .upload_attachment(
                problem_id,
                AttachmentKind::Sample,
                "sample.txt".into(),
                Some("text/plain".into()),
                Bytes::from_static(b"sample"),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("fully scoped admin uploads attachment");
        service
            .upload_testdata(
                problem_id,
                testdata_zip("sample", b"test data"),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("fully scoped admin uploads test data");

        sqlx::query("UPDATE contests SET deleted_at = now() WHERE id IN ($1, $2)")
            .bind(first_contest_id)
            .bind(second_contest_id)
            .execute(&pool)
            .await
            .expect("soft-delete contests");
        assert!(
            service
                .upload_attachment(
                    problem_id,
                    AttachmentKind::Supplement,
                    "guide.pdf".into(),
                    Some("application/pdf".into()),
                    Bytes::from_static(b"guide"),
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    &storage,
                )
                .await
                .is_err()
        );
        assert!(
            service.download_attachment(problem_id, attachment.id, &actor, &storage).await.is_err()
        );
        assert!(service.download_testdata(problem_id, &actor, &storage).await.is_err());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn testdata_versions_are_immutable_and_current_pointer_is_downloadable(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('testdata-admin', 'test-hash', 'Testdata Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert admin");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('testdata', 'Test Data', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let actor = AuthUser {
            id: user_id,
            username: "testdata-admin".into(),
            display_name: "Testdata Admin".into(),
            user_type: UserType::SuperAdmin,
            roles: vec!["SUPER_ADMIN".into()],
            password_reset_required: false,
        };
        let memory = Arc::new(MemoryStorage::default());
        let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
        let service = ProblemService::new(pool.clone());
        let first_content = testdata_zip("1", b"first-version");
        let first = service
            .upload_testdata(
                problem_id,
                first_content.clone(),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("upload first test-data version");
        let second_content = testdata_zip("1", b"second-version");
        let second = service
            .upload_testdata(
                problem_id,
                second_content.clone(),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("upload second test-data version");
        assert_eq!((first.version, second.version), (1, 2));
        assert_eq!((first.case_count, second.case_count), (Some(1), Some(1)));
        assert_ne!(first.sha256, second.sha256);
        let versions = sqlx::query_as::<_, (i32, String)>(
            "SELECT version, object_key FROM problem_testdata_versions WHERE problem_id = $1 ORDER BY version",
        )
        .bind(problem_id)
        .fetch_all(&pool)
        .await
        .expect("load immutable test-data history");
        assert_eq!(versions.len(), 2);
        assert_ne!(versions[0].1, versions[1].1);
        assert_eq!(
            memory
                .get("problems-test", &versions[0].1)
                .await
                .expect("first version object remains"),
            first_content
        );
        let download = service
            .download_testdata(problem_id, &actor, &storage)
            .await
            .expect("download current test data");
        assert!(download.filename.ends_with("v2.zip"));
        let download_bytes = axum::body::to_bytes(download.content, 8 * 1024 * 1024)
            .await
            .expect("read current test-data download");
        assert_eq!(download_bytes, second_content);
        let current = sqlx::query_as::<_, (i32, String)>(
            "SELECT testdata_version, testdata_sha256 FROM problems WHERE id = $1",
        )
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("load current test-data pointer");
        assert_eq!(current, (second.version, second.sha256));
        let authoritative = service
            .current_testdata_reference(problem_id)
            .await
            .expect("load authoritative test-data reference");
        assert_eq!(authoritative.version, 2);
        assert_eq!(authoritative.object_key, versions[1].1);
        assert_eq!(authoritative.sha256, current.1);
        assert_eq!(authoritative.case_count, Some(1));
        let history = service
            .list_testdata_versions(problem_id, &actor)
            .await
            .expect("list test-data history");
        assert_eq!(history.iter().map(|item| item.version).collect::<Vec<_>>(), vec![2, 1]);
        assert!(history[0].active);
        assert!(!history[1].active);
        let first_download = service
            .download_testdata_version(problem_id, 1, &actor, &storage)
            .await
            .expect("download first test-data version");
        assert!(first_download.filename.ends_with("v1.zip"));
        let first_download_bytes = axum::body::to_bytes(first_download.content, 8 * 1024 * 1024)
            .await
            .expect("read first test-data version download");
        assert_eq!(first_download_bytes, first_content);
        let activated = service
            .activate_testdata_version(problem_id, 1, 2, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("activate historical test-data version");
        assert_eq!(activated.version, 1);
        assert!(activated.active);
        assert!(
            service
                .activate_testdata_version(
                    problem_id,
                    2,
                    2,
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                )
                .await
                .is_err()
        );
        let third_content = testdata_zip("1", b"third-version");
        let third = service
            .upload_testdata(
                problem_id,
                third_content.clone(),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("upload after activating historical version");
        assert_eq!(third.version, 3);
        let third_download = service
            .download_testdata(problem_id, &actor, &storage)
            .await
            .expect("download third version");
        let third_download_bytes = axum::body::to_bytes(third_download.content, 8 * 1024 * 1024)
            .await
            .expect("read third test-data download");
        assert_eq!(third_download_bytes, third_content);
        sqlx::query("UPDATE problems SET testdata_sha256 = $2 WHERE id = $1")
            .bind(problem_id)
            .bind("0".repeat(64))
            .execute(&pool)
            .await
            .expect("simulate inconsistent compatibility pointer");
        assert!(service.current_testdata_reference(problem_id).await.is_err());
    }
}
