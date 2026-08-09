use super::*;

impl ProblemService {
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
                sqlx::query_as::<_, ProblemRow>(sqlx::AssertSqlSafe(sql)).bind(problem_id).fetch_one(&self.database).await
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
