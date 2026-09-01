use super::*;

/// A test-data archive staged to a temporary file by the upload handler.
///
/// The 256 MiB request budget is never held in memory: the handler streams the
/// multipart field to disk while hashing, and the service streams the file to
/// object storage from here.
pub struct StagedTestdataUpload {
    pub path: std::path::PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

const MAX_TESTDATA_BYTES: u64 = 256 * 1024 * 1024;

impl ProblemService {
    pub async fn upload_testdata(
        &self,
        problem_id: i64,
        upload: StagedTestdataUpload,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<ProblemTestdataResponse, AppError> {
        require_positive_id(problem_id)?;
        if upload.bytes == 0 || upload.bytes > MAX_TESTDATA_BYTES {
            return Err(AppError::validation("file", "must contain between 1 byte and 256 MiB"));
        }
        let archive = testdata_archive::validate_file(&upload.path).await?;
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
        let sha256 = upload.sha256;
        let object_key = keys::testdata(problem_id, version);
        storage
            .backend()
            .put_file(storage.problem_bucket(), &object_key, Some("application/zip"), &upload.path)
            .await
            .map_err(|error| AppError::internal("upload problem test data", error))?;
        let persisted = self
            .persist_testdata(
                problem_id,
                previous_version,
                version,
                archive.case_count,
                i64::try_from(upload.bytes)
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
