use std::net::IpAddr;

use project_balloon_contracts::{JUDGE_TASK_SCHEMA_VERSION, JudgeTask};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::AuthUser,
    object_storage::{ObjectStorageHandle, keys},
    object_storage_cleanup::defer_failed_cleanup,
    pagination::PageResponse,
};

use crate::features::submissions::{
    SubmissionService,
    model::{
        JudgementDetail, PracticeProblemStatus, PracticeSubmissionDetail,
        PracticeSubmissionSummary, SubmitResponse, ValidatedSubmission,
        ValidatedSubmissionListQuery, source_fingerprint, source_similarity_signature,
    },
    service::{language_multiplier, parse_judge_mode},
};

const PRACTICE_LIMIT_PER_MINUTE: i64 = 20;

#[derive(sqlx::FromRow)]
struct PracticeContext {
    time_limit_ms: i32,
    memory_limit_mb: i32,
    output_limit_kb: i32,
    languages: String,
    testdata_version: i32,
    testdata_object_key: String,
    testdata_sha256: String,
    judge_mode: String,
    interactor_object_key: Option<String>,
    interactor_sha256: Option<String>,
}

const PRACTICE_CONTEXT_SQL: &str = r#"
SELECT problem.time_limit_ms,problem.memory_limit_mb,problem.output_limit_kb,
       problem.languages,version.version AS testdata_version,
       version.object_key AS testdata_object_key,version.sha256 AS testdata_sha256,
       problem.judge_mode,problem.interactor_object_key,problem.interactor_sha256
FROM problem_bank_entries bank
JOIN problems problem ON problem.id=bank.problem_id AND problem.deleted_at IS NULL
JOIN problem_testdata_versions version ON version.problem_id=problem.id
 AND version.version=problem.testdata_version
 AND version.object_key=problem.testdata_object_key
 AND version.sha256=problem.testdata_sha256
WHERE bank.problem_id=$1 AND bank.visibility='PUBLIC'
"#;

impl SubmissionService {
    pub async fn submit_practice(
        &self,
        command: ValidatedSubmission,
        training_enrollment_id: Option<i64>,
        virtual_session_id: Option<i64>,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<SubmitResponse, AppError> {
        let context = load_context_pool(&self.database, command.problem_id).await?;
        require_language(&context.languages, &command.language)?;
        validate_enrollment_pool(
            &self.database,
            training_enrollment_id,
            command.problem_id,
            actor.id,
        )
        .await?;
        validate_virtual_session_pool(
            &self.database,
            virtual_session_id,
            command.problem_id,
            actor.id,
        )
        .await?;
        let source_sha256 = hex::encode(Sha256::digest(&command.source));
        let fingerprint = source_fingerprint(&command.source);
        let similarity = source_similarity_signature(&command.source);
        let object_key = keys::practice_submission_source(actor.id, command.extension);
        let content_type = if command.language == "output" {
            Some("application/zip")
        } else {
            Some("text/plain; charset=utf-8")
        };
        storage
            .backend()
            .put(storage.source_bucket(), &object_key, content_type, command.source.clone())
            .await
            .map_err(|e| AppError::internal("upload practice source", e).with_user_id(actor.id))?;
        let persisted = self
            .persist_practice(
                &command,
                training_enrollment_id,
                virtual_session_id,
                actor,
                request_ip,
                &context,
                &object_key,
                &source_sha256,
                &fingerprint,
                similarity.simhash,
                similarity.token_count,
            )
            .await;
        if persisted.is_err()
            && let Err(error) = storage.backend().delete(storage.source_bucket(), &object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.source_bucket(),
                &object_key,
                "PRACTICE_SOURCE_UPLOAD_COMPENSATION",
                error.to_string(),
            )
            .await;
        }
        persisted
    }

    // Practice submission fields are kept explicit to preserve validation boundaries.
    #[allow(clippy::too_many_arguments)]
    async fn persist_practice(
        &self,
        command: &ValidatedSubmission,
        training_enrollment_id: Option<i64>,
        virtual_session_id: Option<i64>,
        actor: &AuthUser,
        request_ip: IpAddr,
        _preflight: &PracticeContext,
        object_key: &str,
        source_sha256: &str,
        fingerprint: &str,
        simhash: i64,
        token_count: i32,
    ) -> Result<SubmitResponse, AppError> {
        let mut tx = self.database.begin().await.map_err(|e| {
            AppError::internal("begin practice submission", e).with_user_id(actor.id)
        })?;
        let context = load_context_tx(&mut tx, command.problem_id).await?;
        require_language(&context.languages, &command.language)?;
        validate_enrollment_tx(&mut tx, training_enrollment_id, command.problem_id, actor.id)
            .await?;
        validate_virtual_session_tx(&mut tx, virtual_session_id, command.problem_id, actor.id)
            .await?;
        enforce_practice_rate_limit(&mut tx, actor.id).await?;
        let team_id = sqlx::query_scalar::<_, i64>(
            "SELECT team_id FROM team_accounts WHERE user_id=$1 ORDER BY team_id LIMIT 1",
        )
        .bind(actor.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::internal("load optional practice team", e).with_user_id(actor.id))?;
        let (submission_id, submitted_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            INSERT INTO submissions
                (contest_id,problem_id,team_id,language,source_object_key,source_size_bytes,
                 source_sha256,source_fingerprint,source_simhash,source_token_count,status,
                 submission_scope,participant_user_id,training_enrollment_id,virtual_session_id)
            VALUES(NULL,$1,$2,$3,$4,$5,$6,$7,$8,$9,'PENDING','PRACTICE',$10,$11,$12)
            RETURNING id,submitted_at
            "#,
        )
        .bind(command.problem_id)
        .bind(team_id)
        .bind(&command.language)
        .bind(object_key)
        .bind(
            i32::try_from(command.source.len())
                .map_err(|e| AppError::internal("convert source size", e).with_user_id(actor.id))?,
        )
        .bind(source_sha256)
        .bind(fingerprint)
        .bind(simhash)
        .bind(token_count)
        .bind(actor.id)
        .bind(training_enrollment_id)
        .bind(virtual_session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("insert practice submission", e).with_user_id(actor.id))?;
        let judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements(id,submission_id) VALUES($1,$2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AppError::internal("insert practice judgement", e)
                    .with_submission_id(submission_id)
                    .with_judgement_id(judgement_id)
                    .with_user_id(actor.id)
            })?;
        let task = JudgeTask {
            schema_version: JUDGE_TASK_SCHEMA_VERSION,
            judgement_id,
            submission_id,
            problem_id: command.problem_id,
            testdata_version: context.testdata_version,
            testdata_object_key: context.testdata_object_key,
            testdata_sha256: context.testdata_sha256,
            source_object_key: object_key.to_owned(),
            source_sha256: source_sha256.to_owned(),
            language: command.language.clone(),
            time_limit_ms: context.time_limit_ms,
            memory_limit_mb: context.memory_limit_mb,
            output_limit_kb: context.output_limit_kb,
            language_multiplier: language_multiplier(&command.language),
            judge_mode: parse_judge_mode(&context.judge_mode)?,
            interactor_object_key: context.interactor_object_key,
            interactor_sha256: context.interactor_sha256,
        };
        task.validate().map_err(|e| {
            AppError::internal("validate practice judge task", e)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
        })?;
        let payload = serde_json::to_string(&task).map_err(|e| {
            AppError::internal("serialize practice judge task", e)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
        })?;
        sqlx::query(
            "INSERT INTO submission_outbox(judgement_id,submission_id,payload) VALUES($1,$2,$3)",
        )
        .bind(judgement_id)
        .bind(submission_id)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::internal("enqueue practice judge task", e)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
                .with_user_id(actor.id)
        })?;
        sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES($1,'PRACTICE_SUBMISSION_CREATED','SUBMISSION',$2,$3,'success')").bind(actor.id).bind(submission_id.to_string()).bind(request_ip.to_string()).execute(&mut *tx).await.map_err(|e|AppError::internal("audit practice submission",e).with_submission_id(submission_id).with_user_id(actor.id))?;
        tx.commit().await.map_err(|e| {
            AppError::internal("commit practice submission", e)
                .with_submission_id(submission_id)
                .with_user_id(actor.id)
        })?;
        Ok(SubmitResponse { submission_id, judgement_id, status: "PENDING", submitted_at })
    }

    pub async fn list_practice(
        &self,
        actor: &AuthUser,
        query: ValidatedSubmissionListQuery,
    ) -> Result<PageResponse<PracticeSubmissionSummary>, AppError> {
        if query.team_id.is_some() {
            return Err(AppError::validation(
                "teamId",
                "is not supported for practice submissions",
            ));
        }
        let total=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM submissions WHERE submission_scope='PRACTICE' AND participant_user_id=$1 AND ($2::bigint IS NULL OR problem_id=$2) AND ($3::text IS NULL OR status=$3) AND ($4::text IS NULL OR verdict=$4) AND ($5::text IS NULL OR language=$5)").bind(actor.id).bind(query.problem_id).bind(query.status.as_deref()).bind(query.verdict.as_deref()).bind(query.language.as_deref()).fetch_one(&self.database).await.map_err(|e|AppError::internal("count practice submissions",e).with_user_id(actor.id))?;
        let rows = sqlx::query_as::<_, PracticeSubmissionSummary>(
            r#"
            SELECT s.id,s.problem_id,p.slug AS problem_slug,p.title AS problem_title,
                   s.training_enrollment_id,s.language,s.source_size_bytes,s.status,
                   s.submitted_at,s.judged_at,j.id AS active_judgement_id,j.verdict,
                   j.total_time_ms,j.peak_memory_kb,
                   CASE WHEN j.verdict='ACCEPTED' THEN 100
                        WHEN j.completed_at IS NOT NULL THEN 0
                        ELSE NULL END AS score
            FROM submissions s
            JOIN problems p
                ON p.id=s.problem_id
            LEFT JOIN judgements j
                ON j.submission_id=s.id AND j.active_marker IS TRUE
            WHERE s.submission_scope='PRACTICE' AND s.participant_user_id=$1
                AND ($2::bigint IS NULL OR s.problem_id=$2)
                AND ($3::text IS NULL OR s.status=$3)
                AND ($4::text IS NULL OR s.verdict=$4)
                AND ($5::text IS NULL OR s.language=$5)
            ORDER BY s.submitted_at DESC,s.id DESC
            LIMIT $6 OFFSET $7
            "#,
        )
        .bind(actor.id)
        .bind(query.problem_id)
        .bind(query.status.as_deref())
        .bind(query.verdict.as_deref())
        .bind(query.language.as_deref())
        .bind(i64::from(query.size))
        .bind(query.offset)
        .fetch_all(&self.database)
        .await
        .map_err(|e| AppError::internal("list practice submissions", e).with_user_id(actor.id))?;
        Ok(PageResponse::new(rows, query.page, query.size, total))
    }

    pub async fn practice_progress(
        &self,
        actor: &AuthUser,
    ) -> Result<Vec<PracticeProblemStatus>, AppError> {
        sqlx::query_as::<_,PracticeProblemStatus>("SELECT user_id,problem_id,attempts,best_score,solved,last_submission_id,solved_at,updated_at FROM practice_problem_progress WHERE user_id=$1 ORDER BY updated_at DESC,problem_id").bind(actor.id).fetch_all(&self.database).await.map_err(|e|AppError::internal("list practice progress",e).with_user_id(actor.id))
    }

    pub async fn practice_detail(
        &self,
        submission_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<PracticeSubmissionDetail, AppError> {
        let summary = sqlx::query_as::<_, PracticeSubmissionSummary>(
            r#"
            SELECT s.id,s.problem_id,p.slug AS problem_slug,p.title AS problem_title,
                   s.training_enrollment_id,s.language,s.source_size_bytes,s.status,
                   s.submitted_at,s.judged_at,j.id AS active_judgement_id,j.verdict,
                   j.total_time_ms,j.peak_memory_kb,
                   CASE WHEN j.verdict='ACCEPTED' THEN 100
                        WHEN j.completed_at IS NOT NULL THEN 0
                        ELSE NULL END AS score
            FROM submissions s
            JOIN problems p
                ON p.id=s.problem_id
            LEFT JOIN judgements j
                ON j.submission_id=s.id AND j.active_marker IS TRUE
            WHERE s.id=$1 AND s.submission_scope='PRACTICE' AND s.participant_user_id=$2
            "#,
        )
        .bind(submission_id)
        .bind(actor.id)
        .fetch_optional(&self.database)
        .await
        .map_err(|e| {
            AppError::internal("load practice submission", e)
                .with_submission_id(submission_id)
                .with_user_id(actor.id)
        })?
        .ok_or_else(|| {
            AppError::not_found("SUBMISSION_NOT_FOUND", "Practice submission not found")
        })?;
        let (key, hash, source_size_bytes, deleted_at) = sqlx::query_as::<
            _,
            (String, Option<String>, i32, Option<OffsetDateTime>),
        >(
            "SELECT source_object_key,source_sha256,source_size_bytes,source_deleted_at FROM submissions WHERE id=$1",
        )
        .bind(submission_id)
        .fetch_one(&self.database)
        .await
        .map_err(|e| AppError::internal("load practice source metadata", e).with_submission_id(submission_id))?;
        let source = if deleted_at.is_some() {
            "[Source expired according to platform retention policy]".into()
        } else if summary.language == "output" {
            "[Output-only ZIP archive]".into()
        } else {
            let expected_source_size = usize::try_from(source_size_bytes).unwrap_or(0);
            if expected_source_size == 0 || expected_source_size > super::model::MAX_SOURCE_BYTES {
                return Err(AppError::conflict(
                    "SUBMISSION_SOURCE_SIZE_MISMATCH",
                    "Stored practice source has an unsupported recorded size",
                ));
            }
            let bytes = storage
                .backend()
                .get_limited(storage.source_bucket(), &key, expected_source_size)
                .await
                .map_err(|e| {
                    AppError::internal("download practice source", e)
                        .with_submission_id(submission_id)
                })?;
            if bytes.len() != expected_source_size {
                return Err(AppError::conflict(
                    "SUBMISSION_SOURCE_SIZE_MISMATCH",
                    "Stored practice source does not match its recorded size",
                ));
            }
            if let Some(expected_hash) = hash.as_deref()
                && hex::encode(Sha256::digest(&bytes)) != expected_hash
            {
                return Err(AppError::conflict(
                    "SUBMISSION_SOURCE_HASH_MISMATCH",
                    "Stored practice source failed integrity verification",
                ));
            }
            String::from_utf8(bytes.to_vec()).map_err(|_| {
                AppError::conflict(
                    "SUBMISSION_SOURCE_INVALID",
                    "Stored practice source is not UTF-8",
                )
            })?
        };
        let judgements=sqlx::query_as::<_,JudgementDetail>("SELECT id,verdict,total_time_ms,peak_memory_kb,compile_log,worker_id,started_at,completed_at,created_at,version,superseded,active_marker IS TRUE AS active,score_milli FROM judgements WHERE submission_id=$1 ORDER BY created_at DESC,id DESC").bind(submission_id).fetch_all(&self.database).await.map_err(|e|AppError::internal("load practice judgements",e).with_submission_id(submission_id))?;
        Ok(PracticeSubmissionDetail { summary, source, source_sha256: hash, judgements })
    }
}

async fn load_context_pool(
    database: &PgPool,
    problem_id: i64,
) -> Result<PracticeContext, AppError> {
    sqlx::query_as(PRACTICE_CONTEXT_SQL)
        .bind(problem_id)
        .fetch_optional(database)
        .await
        .map_err(|e| AppError::internal("validate practice problem", e))?
        .ok_or_else(practice_not_allowed)
}
async fn load_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    problem_id: i64,
) -> Result<PracticeContext, AppError> {
    let query = format!("{PRACTICE_CONTEXT_SQL} FOR SHARE OF bank,problem,version");
    sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(problem_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::internal("revalidate practice problem", e))?
        .ok_or_else(practice_not_allowed)
}
fn require_language(json: &str, language: &str) -> Result<(), AppError> {
    let languages: Vec<String> = serde_json::from_str(json)
        .map_err(|e| AppError::internal("parse practice languages", e))?;
    if languages.iter().any(|v| v == language) {
        Ok(())
    } else {
        Err(AppError::conflict("LANGUAGE_NOT_ALLOWED", "Language is not enabled for this problem"))
    }
}
async fn validate_enrollment_pool(
    database: &PgPool,
    id: Option<i64>,
    problem: i64,
    user: i64,
) -> Result<(), AppError> {
    if let Some(id) = id {
        let valid = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM training_enrollments e
                JOIN training_set_items i
                    ON i.set_id=e.set_id AND i.problem_id=$2
                WHERE e.id=$1 AND e.status IN('ACTIVE','COMPLETED')
                    AND (e.user_id=$3
                        OR EXISTS(
                            SELECT 1 FROM team_accounts a
                            WHERE a.team_id=e.team_id AND a.user_id=$3
                        ))
            )
            "#,
        )
        .bind(id)
        .bind(problem)
        .bind(user)
        .fetch_one(database)
        .await
        .map_err(|e| AppError::internal("validate training enrollment", e).with_user_id(user))?;
        if !valid {
            return Err(AppError::conflict(
                "TRAINING_ENROLLMENT_INVALID",
                "Enrollment does not contain this problem",
            ));
        }
    }
    Ok(())
}
async fn validate_enrollment_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Option<i64>,
    problem: i64,
    user: i64,
) -> Result<(), AppError> {
    if let Some(id) = id {
        let valid = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM training_enrollments e
                JOIN training_set_items i
                    ON i.set_id=e.set_id AND i.problem_id=$2
                WHERE e.id=$1 AND e.status IN('ACTIVE','COMPLETED')
                    AND (e.user_id=$3
                        OR EXISTS(
                            SELECT 1 FROM team_accounts a
                            WHERE a.team_id=e.team_id AND a.user_id=$3
                        ))
            )
            "#,
        )
        .bind(id)
        .bind(problem)
        .bind(user)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::internal("revalidate training enrollment", e).with_user_id(user))?;
        if !valid {
            return Err(AppError::conflict(
                "TRAINING_ENROLLMENT_INVALID",
                "Enrollment does not contain this problem",
            ));
        }
    }
    Ok(())
}
async fn validate_virtual_session_pool(
    database: &PgPool,
    id: Option<i64>,
    problem: i64,
    user: i64,
) -> Result<(), AppError> {
    if let Some(id) = id {
        let valid=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM practice_virtual_sessions s JOIN practice_virtual_items i ON i.session_id=s.id AND i.problem_id=$2 WHERE s.id=$1 AND s.user_id=$3 AND s.archived_at IS NULL AND now()>=s.start_at AND now()<s.end_at)").bind(id).bind(problem).bind(user).fetch_one(database).await.map_err(|e|AppError::internal("validate virtual practice session",e).with_user_id(user))?;
        if !valid {
            return Err(AppError::conflict(
                "VIRTUAL_SESSION_NOT_ACTIVE",
                "Virtual session is inactive or does not contain this problem",
            ));
        }
    }
    Ok(())
}
async fn validate_virtual_session_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Option<i64>,
    problem: i64,
    user: i64,
) -> Result<(), AppError> {
    if let Some(id) = id {
        let valid=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM practice_virtual_sessions s JOIN practice_virtual_items i ON i.session_id=s.id AND i.problem_id=$2 WHERE s.id=$1 AND s.user_id=$3 AND s.archived_at IS NULL AND now()>=s.start_at AND now()<s.end_at)").bind(id).bind(problem).bind(user).fetch_one(&mut **tx).await.map_err(|e|AppError::internal("revalidate virtual practice session",e).with_user_id(user))?;
        if !valid {
            return Err(AppError::conflict(
                "VIRTUAL_SESSION_NOT_ACTIVE",
                "Virtual session is inactive or does not contain this problem",
            ));
        }
    }
    Ok(())
}
async fn enforce_practice_rate_limit(
    tx: &mut Transaction<'_, Postgres>,
    user: i64,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("practice:{user}"))
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::internal("lock practice rate limit", e).with_user_id(user))?;
    let recent=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM submissions WHERE submission_scope='PRACTICE' AND participant_user_id=$1 AND submitted_at>now()-interval '1 minute'").bind(user).fetch_one(&mut **tx).await.map_err(|e|AppError::internal("check practice rate limit",e).with_user_id(user))?;
    if recent >= PRACTICE_LIMIT_PER_MINUTE {
        return Err(AppError::too_many_requests(
            "PRACTICE_SUBMISSION_RATE_LIMITED",
            "Too many practice submissions; try again later",
        ));
    }
    let limits=sqlx::query_as::<_,(i32,i32)>("SELECT daily_submission_limit,concurrent_judging_limit FROM practice_platform_settings WHERE singleton").fetch_one(&mut **tx).await.map_err(|e|AppError::internal("load practice limits",e).with_user_id(user))?;
    let usage=sqlx::query_as::<_,(i64,i64)>("SELECT count(*) FILTER(WHERE submitted_at>=date_trunc('day',now())),count(*) FILTER(WHERE status IN('PENDING','JUDGING')) FROM submissions WHERE submission_scope='PRACTICE' AND participant_user_id=$1").bind(user).fetch_one(&mut **tx).await.map_err(|e|AppError::internal("load practice usage",e).with_user_id(user))?;
    if usage.0 >= i64::from(limits.0) {
        return Err(AppError::too_many_requests(
            "PRACTICE_DAILY_QUOTA_EXCEEDED",
            "Daily practice submission quota is exhausted",
        ));
    }
    if usage.1 >= i64::from(limits.1) {
        return Err(AppError::conflict(
            "PRACTICE_CONCURRENCY_LIMIT",
            "Too many practice submissions are still being judged",
        ));
    }
    Ok(())
}
fn practice_not_allowed() -> AppError {
    AppError::conflict(
        "PRACTICE_SUBMISSION_NOT_ALLOWED",
        "Problem is not public or has no active test data",
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use bytes::Bytes;
    use sqlx::PgPool;

    use crate::features::auth::model::AuthUser;
    use crate::features::submissions::SubmissionService;
    use crate::features::submissions::model::{
        ValidatedSubmission, ValidatedSubmissionListQuery, source_fingerprint,
        source_similarity_signature,
    };
    use crate::object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle};

    use super::{PRACTICE_LIMIT_PER_MINUTE, require_language};

    const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    #[derive(Default)]
    struct MemoryStorage {
        objects: Mutex<HashMap<(String, String), Bytes>>,
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
                .insert((bucket.into(), key.into()), content);
            Ok(())
        }

        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
            self.objects
                .lock()
                .expect("memory storage lock")
                .get(&(bucket.into(), key.into()))
                .cloned()
                .ok_or_else(|| ObjectStorageError::Request("not found".into()))
        }

        async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
            self.objects.lock().expect("memory storage lock").remove(&(bucket.into(), key.into()));
            Ok(())
        }
    }

    async fn seed_practicer(pool: &PgPool) -> AuthUser {
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type)
             VALUES ('practice-user', 'test-hash', 'Practice User', 'INDIVIDUAL') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("insert practice user");
        AuthUser {
            id: user_id,
            username: "practice-user".into(),
            display_name: "Practice User".into(),
            user_type: crate::features::auth::model::UserType::Individual,
            permissions: Vec::new(),
            password_reset_required: false,
        }
    }

    async fn seed_public_problem(pool: &PgPool, slug: &str, languages: &str) -> i64 {
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, languages, testdata_version, testdata_object_key,
                                    testdata_sha256)
             VALUES ($1, $2, $3, 1, 'problems/practice/v1.zip', $4) RETURNING id",
        )
        .bind(slug)
        .bind("Practice Problem")
        .bind(languages)
        .bind("e".repeat(64))
        .fetch_one(pool)
        .await
        .expect("insert practice problem");
        sqlx::query(
            "INSERT INTO problem_testdata_versions (problem_id, version, object_key, sha256, bytes, case_count)
             VALUES ($1, 1, 'problems/practice/v1.zip', $2, 100, 1)",
        )
        .bind(problem_id)
        .bind("e".repeat(64))
        .execute(pool)
        .await
        .expect("insert practice testdata version");
        sqlx::query(
            "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at)
             VALUES ($1, 'PUBLIC', '[]', now())",
        )
        .bind(problem_id)
        .execute(pool)
        .await
        .expect("publish practice problem");
        problem_id
    }

    fn practice_command(problem_id: i64, language: &str) -> ValidatedSubmission {
        ValidatedSubmission {
            problem_id,
            language: language.to_owned(),
            extension: ".cpp",
            source: Bytes::from_static(b"#include <iostream>\nint main(){return 0;}\n"),
        }
    }

    async fn practice_harness(
        pool: &PgPool,
        languages: &str,
    ) -> (AuthUser, ObjectStorageHandle, Arc<MemoryStorage>, i64) {
        let actor = seed_practicer(pool).await;
        let problem_id = seed_public_problem(pool, "practice-harness", languages).await;
        let backend = Arc::new(MemoryStorage::default());
        let storage =
            ObjectStorageHandle::with_buckets(backend.clone(), "problems".into(), "sources".into());
        (actor, storage, backend, problem_id)
    }

    #[test]
    fn require_language_accepts_only_listed_languages() {
        assert!(require_language(r#"["cpp","java"]"#, "cpp").is_ok());
        let rejected = require_language(r#"["cpp"]"#, "brainfuck").expect_err("unknown language");
        assert_eq!(rejected.code(), "LANGUAGE_NOT_ALLOWED");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn practice_submission_persists_task_simhash_and_audit(pool: PgPool) {
        let (actor, storage, backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        let command = practice_command(problem_id, "cpp");
        let fingerprint = source_fingerprint(&command.source);
        let similarity = source_similarity_signature(&command.source);
        let service = SubmissionService::new(pool.clone());

        let response = service
            .submit_practice(command, None, None, &actor, LOCALHOST, &storage)
            .await
            .expect("practice submission accepted");

        let row = sqlx::query_as::<_, (String, String, String, Option<i64>, Option<i32>, i64)>(
            "SELECT status, submission_scope, source_fingerprint, source_simhash,
                    source_token_count, count(j.id)
             FROM submissions s
             LEFT JOIN judgements j ON j.submission_id = s.id
             WHERE s.id = $1 GROUP BY s.id, s.status, s.submission_scope,
                   s.source_fingerprint, s.source_simhash, s.source_token_count",
        )
        .bind(response.submission_id)
        .fetch_one(&pool)
        .await
        .expect("load practice submission");
        assert_eq!((row.0.as_str(), row.1.as_str()), ("PENDING", "PRACTICE"));
        assert_eq!(row.2, fingerprint);
        assert_eq!(row.3, Some(similarity.simhash));
        assert_eq!(row.4, Some(similarity.token_count));
        assert_eq!(row.5, 1, "a practice submission starts with one active judgement");
        assert_eq!(
            response.judgement_id,
            uuid::Uuid::parse_str(
                &sqlx::query_scalar::<_, String>(
                    "SELECT id::text FROM judgements WHERE submission_id = $1",
                )
                .bind(response.submission_id)
                .fetch_one(&pool)
                .await
                .expect("load judgement id"),
            )
            .expect("judgement id parses")
        );

        let payload = sqlx::query_scalar::<_, String>(
            "SELECT payload FROM submission_outbox WHERE submission_id = $1",
        )
        .bind(response.submission_id)
        .fetch_one(&pool)
        .await
        .expect("load judge task payload");
        let task: project_balloon_contracts::JudgeTask =
            serde_json::from_str(&payload).expect("payload is a valid judge task");
        assert_eq!(task.judgement_id, response.judgement_id);
        assert_eq!(task.submission_id, response.submission_id);
        assert_eq!(task.problem_id, problem_id);

        let audits = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'PRACTICE_SUBMISSION_CREATED'",
        )
        .fetch_one(&pool)
        .await
        .expect("count practice audits");
        assert_eq!(audits, 1);

        let objects = backend.objects.lock().expect("memory storage lock");
        assert_eq!(objects.len(), 1, "exactly one source object must be stored");
        for ((bucket, key), content) in objects.iter() {
            assert_eq!(bucket, "sources");
            assert!(key.starts_with(&format!("practice-submissions/{}/", actor.id)));
            assert_eq!(content.as_ref(), b"#include <iostream>\nint main(){return 0;}\n");
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn disabled_languages_are_rejected_before_any_side_effect(pool: PgPool) {
        let (actor, storage, backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        let service = SubmissionService::new(pool.clone());

        let rejected = service
            .submit_practice(
                practice_command(problem_id, "java"),
                None,
                None,
                &actor,
                LOCALHOST,
                &storage,
            )
            .await
            .expect_err("java must be rejected for a cpp-only problem");
        assert_eq!(rejected.code(), "LANGUAGE_NOT_ALLOWED");

        let submissions = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM submissions")
            .fetch_one(&pool)
            .await
            .expect("count submissions");
        assert_eq!(submissions, 0, "no submission may survive a language rejection");
        assert!(
            backend.objects.lock().expect("memory storage lock").is_empty(),
            "a rejected language must not reach object storage"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn practice_rate_limit_bounds_the_per_minute_burst(pool: PgPool) {
        let (actor, storage, _backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        let service = SubmissionService::new(pool.clone());
        // Lift the concurrent-judging ceiling (default 3) so the burst below
        // is bounded by the per-minute budget instead.
        sqlx::query("UPDATE practice_platform_settings SET concurrent_judging_limit = 20")
            .execute(&pool)
            .await
            .expect("raise concurrent practice budget");

        for _ in 0..PRACTICE_LIMIT_PER_MINUTE {
            service
                .submit_practice(
                    practice_command(problem_id, "cpp"),
                    None,
                    None,
                    &actor,
                    LOCALHOST,
                    &storage,
                )
                .await
                .expect("submissions within the burst budget");
        }
        let over_limit = service
            .submit_practice(
                practice_command(problem_id, "cpp"),
                None,
                None,
                &actor,
                LOCALHOST,
                &storage,
            )
            .await
            .expect_err("the burst beyond the budget must be rejected");
        assert_eq!(over_limit.code(), "PRACTICE_SUBMISSION_RATE_LIMITED");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn practice_listings_round_trip_the_submitted_work(pool: PgPool) {
        let (actor, storage, _backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        let service = SubmissionService::new(pool.clone());
        let first = service
            .submit_practice(
                practice_command(problem_id, "cpp"),
                None,
                None,
                &actor,
                LOCALHOST,
                &storage,
            )
            .await
            .expect("first practice submission");

        let page = service
            .list_practice(
                &actor,
                ValidatedSubmissionListQuery {
                    team_id: None,
                    problem_id: Some(problem_id),
                    status: None,
                    verdict: None,
                    language: None,
                    page: 1,
                    size: 10,
                    offset: 0,
                },
            )
            .await
            .expect("list practice submissions");
        assert_eq!(page.total_elements, 1);
        assert_eq!(page.content[0].id, first.submission_id);

        let detail = service
            .practice_detail(first.submission_id, &actor, &storage)
            .await
            .expect("practice detail");
        assert_eq!(detail.summary.id, first.submission_id);
        assert_eq!(detail.summary.active_judgement_id, Some(first.judgement_id));

        let progress = service.practice_progress(&actor).await.expect("practice progress");
        assert!(progress.is_empty(), "unjudged submissions do not produce progress rows");

        let foreign = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type)
             VALUES ('other-user', 'test-hash', 'Other User', 'INDIVIDUAL') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert other user");
        let stranger = AuthUser {
            id: foreign,
            username: "other-user".into(),
            display_name: "Other User".into(),
            user_type: crate::features::auth::model::UserType::Individual,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        let hidden = service
            .practice_detail(first.submission_id, &stranger, &storage)
            .await
            .expect_err("other users cannot see foreign practice submissions");
        assert_eq!(hidden.code(), "SUBMISSION_NOT_FOUND");
    }

    /// Wraps [`MemoryStorage`] with injection points so the upload and its
    /// compensation path can be forced to fail.
    #[derive(Default)]
    struct FaultyStorage {
        inner: MemoryStorage,
        fail_put: bool,
        fail_delete: bool,
    }

    #[async_trait]
    impl ObjectStorage for FaultyStorage {
        async fn check_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError> {
            self.inner.check_bucket(bucket).await
        }

        async fn put(
            &self,
            bucket: &str,
            key: &str,
            content_type: Option<&str>,
            content: Bytes,
        ) -> Result<(), ObjectStorageError> {
            if self.fail_put {
                return Err(ObjectStorageError::Request("put rejected".into()));
            }
            self.inner.put(bucket, key, content_type, content).await
        }

        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
            self.inner.get(bucket, key).await
        }

        async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
            if self.fail_delete {
                return Err(ObjectStorageError::Request("delete rejected".into()));
            }
            self.inner.delete(bucket, key).await
        }
    }

    /// Fills the per-minute practice budget directly so the next submission
    /// fails inside `persist_practice` — after the object upload succeeded.
    async fn exhaust_practice_minute_budget(pool: &PgPool, actor: &AuthUser, problem_id: i64) {
        sqlx::query(
            "INSERT INTO submissions (contest_id, problem_id, language, source_object_key,
                                      source_size_bytes, source_sha256, status,
                                      submission_scope, participant_user_id)
             SELECT NULL, $2, 'cpp', 'sources/filler.cpp', 1, $1, 'PENDING', 'PRACTICE', $3
             FROM generate_series(1, $4)",
        )
        .bind("f".repeat(64))
        .bind(problem_id)
        .bind(actor.id)
        .bind(PRACTICE_LIMIT_PER_MINUTE)
        .execute(pool)
        .await
        .expect("fill practice minute budget");
    }

    async fn compensation_cleanup_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM object_storage_cleanup_tasks WHERE reason='PRACTICE_SOURCE_UPLOAD_COMPENSATION'",
        )
        .fetch_one(pool)
        .await
        .expect("load compensation cleanup tasks")
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn failed_persistence_compensates_by_deleting_the_uploaded_object(pool: PgPool) {
        let (actor, _storage, backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        sqlx::query("UPDATE practice_platform_settings SET concurrent_judging_limit = 20")
            .execute(&pool)
            .await
            .expect("raise concurrent practice budget");
        exhaust_practice_minute_budget(&pool, &actor, problem_id).await;
        let service = SubmissionService::new(pool.clone());

        let rejected = service
            .submit_practice(
                practice_command(problem_id, "cpp"),
                None,
                None,
                &actor,
                LOCALHOST,
                &_storage,
            )
            .await
            .expect_err("the over-budget submission must be rejected");
        assert_eq!(rejected.code(), "PRACTICE_SUBMISSION_RATE_LIMITED");
        assert!(
            backend.objects.lock().expect("memory storage lock").is_empty(),
            "the uploaded source must be compensated away"
        );
        assert_eq!(compensation_cleanup_count(&pool).await, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn failed_compensation_is_deferred_to_the_cleanup_worker(pool: PgPool) {
        let (actor, _storage, backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        sqlx::query("UPDATE practice_platform_settings SET concurrent_judging_limit = 20")
            .execute(&pool)
            .await
            .expect("raise concurrent practice budget");
        exhaust_practice_minute_budget(&pool, &actor, problem_id).await;
        let service = SubmissionService::new(pool.clone());
        let faulty = Arc::new(FaultyStorage {
            inner: MemoryStorage::default(),
            fail_put: false,
            fail_delete: true,
        });
        let storage =
            ObjectStorageHandle::with_buckets(faulty.clone(), "problems".into(), "sources".into());

        let rejected = service
            .submit_practice(
                practice_command(problem_id, "cpp"),
                None,
                None,
                &actor,
                LOCALHOST,
                &storage,
            )
            .await
            .expect_err("the over-budget submission must be rejected");
        assert_eq!(rejected.code(), "PRACTICE_SUBMISSION_RATE_LIMITED");
        assert_eq!(compensation_cleanup_count(&pool).await, 1);
        let _unused = backend;
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn failed_uploads_leave_no_object_and_no_submission(pool: PgPool) {
        let (actor, _storage, backend, problem_id) = practice_harness(&pool, r#"["cpp"]"#).await;
        let service = SubmissionService::new(pool.clone());
        let faulty = Arc::new(FaultyStorage {
            inner: MemoryStorage::default(),
            fail_put: true,
            fail_delete: false,
        });
        let storage =
            ObjectStorageHandle::with_buckets(faulty.clone(), "problems".into(), "sources".into());

        let rejected = service
            .submit_practice(
                practice_command(problem_id, "cpp"),
                None,
                None,
                &actor,
                LOCALHOST,
                &storage,
            )
            .await
            .expect_err("a failed upload cannot submit");
        assert_eq!(rejected.code(), "INTERNAL_ERROR");
        let submissions: i64 = sqlx::query_scalar("SELECT count(*) FROM submissions")
            .fetch_one(&pool)
            .await
            .expect("count submissions");
        assert_eq!(submissions, 0);
        let _unused = backend;
    }
}
