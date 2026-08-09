use std::net::IpAddr;

use project_balloon_contracts::{JUDGE_TASK_SCHEMA_VERSION, JudgeMode, JudgeTask};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::{
        auth::model::{AuthUser, UserType},
        scoreboard,
    },
    object_storage::{ObjectStorageHandle, keys},
    object_storage_cleanup::defer_failed_cleanup,
};

use super::model::{
    RejudgeRequest, RejudgeResponse, SubmitResponse, ValidatedSubmission, source_fingerprint,
    source_similarity_signature,
};

const SUBMISSION_LIMIT_PER_MINUTE: i64 = 20;

#[derive(sqlx::FromRow)]
struct SubmissionContext {
    team_id: i64,
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

#[derive(sqlx::FromRow)]
struct RejudgeContext {
    contest_status: String,
    team_id: i64,
    problem_id: i64,
    language: String,
    source_object_key: String,
    source_sha256: Option<String>,
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
    active_judgement_id: Uuid,
    active_completed_at: Option<OffsetDateTime>,
}

pub struct SubmissionService {
    pub(super) database: PgPool,
}

impl SubmissionService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn submit(
        &self,
        contest_id: i64,
        command: ValidatedSubmission,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<SubmitResponse, AppError> {
        if contest_id <= 0 {
            return Err(AppError::validation("contestId", "must be positive"));
        }
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team account can create submissions",
            ));
        }
        let preflight =
            load_context_pool(&self.database, contest_id, command.problem_id, actor.id).await?;
        require_language(&preflight.languages, &command.language)?;

        let source_sha256 = hex::encode(Sha256::digest(&command.source));
        let source_fingerprint = source_fingerprint(&command.source);
        let similarity = source_similarity_signature(&command.source);
        let source_object_key =
            keys::submission_source(contest_id, preflight.team_id, command.extension);
        storage
            .backend()
            .put(
                storage.source_bucket(),
                &source_object_key,
                Some("text/plain; charset=utf-8"),
                command.source.clone(),
            )
            .await
            .map_err(|error| AppError::internal("upload submission source", error))?;

        let persisted = self
            .persist(
                contest_id,
                command.problem_id,
                &command.language,
                i32::try_from(command.source.len())
                    .map_err(|error| AppError::internal("convert source size", error))?,
                &source_object_key,
                &source_sha256,
                &source_fingerprint,
                similarity.simhash,
                similarity.token_count,
                actor,
                request_ip,
            )
            .await;
        if persisted.is_err()
            && let Err(cleanup_error) =
                storage.backend().delete(storage.source_bucket(), &source_object_key).await
        {
            defer_failed_cleanup(
                &self.database,
                storage.source_bucket(),
                &source_object_key,
                "SUBMISSION_SOURCE_UPLOAD_COMPENSATION",
                cleanup_error.to_string(),
            )
            .await;
        }
        persisted
    }

    pub async fn rejudge(
        &self,
        contest_id: i64,
        submission_id: i64,
        request: RejudgeRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<RejudgeResponse, AppError> {
        self.rejudge_internal(contest_id, submission_id, request, actor, request_ip, None).await
    }

    pub(crate) async fn rejudge_batch_item(
        &self,
        contest_id: i64,
        submission_id: i64,
        expected_judgement_id: Uuid,
        actor: &AuthUser,
        batch_item_id: i64,
    ) -> Result<RejudgeResponse, AppError> {
        self.rejudge_internal(
            contest_id,
            submission_id,
            RejudgeRequest { expected_judgement_id },
            actor,
            "0.0.0.0".parse().map_err(|error| AppError::internal("parse batch audit IP", error))?,
            Some(batch_item_id),
        )
        .await
    }

    async fn rejudge_internal(
        &self,
        contest_id: i64,
        submission_id: i64,
        request: RejudgeRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
        batch_item_id: Option<i64>,
    ) -> Result<RejudgeResponse, AppError> {
        if contest_id <= 0 || submission_id <= 0 {
            return Err(rejudge_submission_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin submission rejudge", error))?;
        if !actor.is_super_admin() {
            let assigned = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM contest_management_assignments
                    WHERE contest_id = $1 AND user_id = $2
                )
                "#,
            )
            .bind(contest_id)
            .bind(actor.id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("check rejudge administrator scope", error))?;
            if !assigned {
                return Err(rejudge_submission_not_found());
            }
        }
        if let Some(batch_item_id) = batch_item_id {
            let existing = sqlx::query_as::<_, (Uuid, OffsetDateTime, Uuid)>(
                r#"
                SELECT judgement.id, judgement.created_at, item.old_judgement_id
                FROM judgements judgement
                JOIN batch_rejudge_items item ON item.id = judgement.batch_rejudge_item_id
                WHERE judgement.batch_rejudge_item_id = $1
                  AND judgement.submission_id = $2
                "#,
            )
            .bind(batch_item_id)
            .bind(submission_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("recover completed batch rejudge item", error))?;
            if let Some((judgement_id, queued_at, previous_judgement_id)) = existing {
                transaction
                    .commit()
                    .await
                    .map_err(|error| AppError::internal("commit recovered batch rejudge", error))?;
                return Ok(RejudgeResponse {
                    submission_id,
                    previous_judgement_id,
                    judgement_id,
                    status: "PENDING",
                    queued_at,
                });
            }
        }
        let context = sqlx::query_as::<_, RejudgeContext>(
            r#"
            SELECT contest.status AS contest_status,
                   submission.team_id,
                   submission.problem_id,
                   submission.language,
                   submission.source_object_key,
                   submission.source_sha256,
                   problem.time_limit_ms,
                   problem.memory_limit_mb,
                   problem.output_limit_kb,
                   problem.languages,
                   version.version AS testdata_version,
                   version.object_key AS testdata_object_key,
                   version.sha256 AS testdata_sha256,
                   problem.judge_mode,
                   problem.interactor_object_key,
                   problem.interactor_sha256,
                   judgement.id AS active_judgement_id,
                   judgement.completed_at AS active_completed_at
            FROM submissions submission
            JOIN contests contest
              ON contest.id = submission.contest_id AND contest.deleted_at IS NULL
            JOIN contest_problems assignment
              ON assignment.contest_id = contest.id
             AND assignment.problem_id = submission.problem_id
            JOIN problems problem
              ON problem.id = submission.problem_id AND problem.deleted_at IS NULL
            JOIN problem_testdata_versions version
              ON version.problem_id = problem.id
             AND version.version = problem.testdata_version
             AND version.object_key = problem.testdata_object_key
             AND version.sha256 = problem.testdata_sha256
            JOIN judgements judgement
              ON judgement.submission_id = submission.id
             AND judgement.active_marker IS TRUE
            WHERE submission.id = $1 AND submission.contest_id = $2
            FOR UPDATE OF submission, judgement
            "#,
        )
        .bind(submission_id)
        .bind(contest_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock submission for rejudge", error))?
        .ok_or_else(rejudge_submission_not_found)?;
        if context.contest_status == "ARCHIVED" {
            return Err(AppError::conflict(
                "CONTEST_ARCHIVED",
                "Archived contest submissions cannot be rejudged",
            ));
        }
        if context.active_judgement_id != request.expected_judgement_id {
            return Err(AppError::conflict(
                "JUDGEMENT_VERSION_STALE",
                "The active judgement changed; reload the submission before rejudging",
            ));
        }
        if context.active_completed_at.is_none() {
            return Err(AppError::conflict(
                "JUDGEMENT_NOT_FINAL",
                "Only a completed judgement can be rejudged",
            ));
        }
        require_language(&context.languages, &context.language)?;
        let source_sha256 = context.source_sha256.as_deref().ok_or_else(|| {
            AppError::conflict(
                "SUBMISSION_SOURCE_UNAVAILABLE",
                "The submission has no verified source hash and cannot be rejudged",
            )
        })?;

        sqlx::query(
            r#"
            UPDATE judgements
            SET superseded = true, active_marker = NULL, version = version + 1
            WHERE id = $1 AND active_marker IS TRUE
            "#,
        )
        .bind(context.active_judgement_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("supersede previous judgement", error))?;
        sqlx::query(
            r#"
            UPDATE submission_outbox
            SET status = 'CANCELLED', last_error = 'superseded by rejudge',
                lease_owner = NULL, lease_until = NULL, version = version + 1
            WHERE judgement_id = $1 AND status <> 'SENT'
            "#,
        )
        .bind(context.active_judgement_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("cancel previous judge task", error))?;
        let judgement_id = Uuid::new_v4();
        let queued_at = sqlx::query_scalar::<_, OffsetDateTime>(
            "INSERT INTO judgements (id, submission_id, batch_rejudge_item_id) VALUES ($1, $2, $3) RETURNING created_at",
        )
        .bind(judgement_id)
        .bind(submission_id)
        .bind(batch_item_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("insert rejudge judgement", error))?;
        sqlx::query("UPDATE submissions SET status = 'PENDING', judged_at = NULL WHERE id = $1")
            .bind(submission_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("reset submission for rejudge", error))?;
        scoreboard::rebuild_cell(&mut transaction, contest_id, context.team_id, context.problem_id)
            .await
            .map_err(|error| AppError::internal("rollback scoreboard for rejudge", error))?;
        let task = JudgeTask {
            schema_version: JUDGE_TASK_SCHEMA_VERSION,
            judgement_id,
            submission_id,
            problem_id: context.problem_id,
            testdata_version: context.testdata_version,
            testdata_object_key: context.testdata_object_key,
            testdata_sha256: context.testdata_sha256,
            source_object_key: context.source_object_key,
            source_sha256: source_sha256.to_owned(),
            language: context.language.clone(),
            time_limit_ms: context.time_limit_ms,
            memory_limit_mb: context.memory_limit_mb,
            output_limit_kb: context.output_limit_kb,
            language_multiplier: language_multiplier(&context.language),
            judge_mode: parse_judge_mode(&context.judge_mode)?,
            interactor_object_key: context.interactor_object_key.clone(),
            interactor_sha256: context.interactor_sha256.clone(),
        };
        task.validate().map_err(|error| AppError::internal("validate rejudge task", error))?;
        let payload = serde_json::to_string(&task)
            .map_err(|error| AppError::internal("serialize rejudge task", error))?;
        sqlx::query(
            "INSERT INTO submission_outbox (judgement_id, submission_id, payload) VALUES ($1, $2, $3)",
        )
        .bind(judgement_id)
        .bind(submission_id)
        .bind(payload)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("enqueue rejudge task", error))?;
        for (scope, recipient) in [("TEAM", Some(context.team_id)), ("STAFF", None)] {
            sqlx::query(
                r#"
                INSERT INTO realtime_outbox
                    (event_id, contest_id, event_type, scope, team_id, payload_json)
                VALUES ($1, $2, 'SUBMISSION_REJUDGED', $3, $4, $5)
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(contest_id)
            .bind(scope)
            .bind(recipient)
            .bind(json!({
                "submissionId": submission_id,
                "previousJudgementId": context.active_judgement_id,
                "judgementId": judgement_id,
                "status": "PENDING"
            }))
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("enqueue rejudge realtime event", error))?;
        }
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, request_ip, result)
            VALUES ($1, 'SUBMISSION_REJUDGED', 'SUBMISSION', $2, $3, 'success')
            "#,
        )
        .bind(actor.id)
        .bind(submission_id.to_string())
        .bind(request_ip.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("record rejudge audit", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit submission rejudge", error))?;
        Ok(RejudgeResponse {
            submission_id,
            previous_judgement_id: context.active_judgement_id,
            judgement_id,
            status: "PENDING",
            queued_at,
        })
    }

    // Keeps persistence inputs explicit; the fields map one-to-one to the submission record.
    #[allow(clippy::too_many_arguments)]
    async fn persist(
        &self,
        contest_id: i64,
        problem_id: i64,
        language: &str,
        source_size: i32,
        source_object_key: &str,
        source_sha256: &str,
        source_fingerprint: &str,
        source_simhash: i64,
        source_token_count: i32,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<SubmitResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin submission transaction", error))?;
        let context =
            load_context_transaction(&mut transaction, contest_id, problem_id, actor.id).await?;
        require_language(&context.languages, language)?;
        enforce_rate_limit(&mut transaction, contest_id, context.team_id).await?;

        let (submission_id, submitted_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, source_fingerprint,
                 source_simhash, source_token_count, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'PENDING')
            RETURNING id, submitted_at
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(context.team_id)
        .bind(language)
        .bind(source_object_key)
        .bind(source_size)
        .bind(source_sha256)
        .bind(source_fingerprint)
        .bind(source_simhash)
        .bind(source_token_count)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("insert submission", error))?;
        let judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements (id, submission_id) VALUES ($1, $2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("insert initial judgement", error))?;
        let task = JudgeTask {
            schema_version: JUDGE_TASK_SCHEMA_VERSION,
            judgement_id,
            submission_id,
            problem_id,
            testdata_version: context.testdata_version,
            testdata_object_key: context.testdata_object_key,
            testdata_sha256: context.testdata_sha256,
            source_object_key: source_object_key.to_owned(),
            source_sha256: source_sha256.to_owned(),
            language: language.to_owned(),
            time_limit_ms: context.time_limit_ms,
            memory_limit_mb: context.memory_limit_mb,
            output_limit_kb: context.output_limit_kb,
            language_multiplier: language_multiplier(language),
            judge_mode: parse_judge_mode(&context.judge_mode)?,
            interactor_object_key: context.interactor_object_key.clone(),
            interactor_sha256: context.interactor_sha256.clone(),
        };
        task.validate()
            .map_err(|error| AppError::internal("validate generated judge task", error))?;
        let payload = serde_json::to_string(&task)
            .map_err(|error| AppError::internal("serialize judge task", error))?;
        sqlx::query(
            r#"
            INSERT INTO submission_outbox (judgement_id, submission_id, payload)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(judgement_id)
        .bind(submission_id)
        .bind(payload)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("enqueue judge task", error))?;
        sqlx::query(
            r#"
            INSERT INTO realtime_outbox
                (event_id, contest_id, event_type, scope, team_id, payload_json)
            VALUES ($1, $2, 'SUBMISSION_STATUS_CHANGED', 'TEAM', $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(contest_id)
        .bind(context.team_id)
        .bind(json!({
            "submissionId": submission_id,
            "judgementId": judgement_id,
            "status": "PENDING"
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("enqueue submission realtime event", error))?;
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, request_ip, result)
            VALUES ($1, 'SUBMISSION_CREATED', 'SUBMISSION', $2, $3, 'success')
            "#,
        )
        .bind(actor.id)
        .bind(submission_id.to_string())
        .bind(request_ip.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("record submission audit", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit submission", error))?;
        Ok(SubmitResponse { submission_id, judgement_id, status: "PENDING", submitted_at })
    }
}

pub(super) fn language_multiplier(language: &str) -> f64 {
    match language {
        "java" => 2.0,
        "python" => 3.0,
        "c" | "cpp" => 1.0,
        _ => 1.0,
    }
}

pub(super) fn parse_judge_mode(value: &str) -> Result<JudgeMode, AppError> {
    match value {
        "STANDARD" => Ok(JudgeMode::Standard),
        "INTERACTIVE" => Ok(JudgeMode::Interactive),
        "OUTPUT_ONLY" => Ok(JudgeMode::OutputOnly),
        invalid => Err(AppError::internal("invalid problems.judge_mode", invalid)),
    }
}

const CONTEXT_QUERY: &str = r#"
    SELECT
        account.team_id,
        problem.time_limit_ms,
        problem.memory_limit_mb,
        problem.output_limit_kb,
        problem.languages,
        problem.judge_mode,
        problem.interactor_object_key,
        problem.interactor_sha256,
        version.version AS testdata_version,
        version.object_key AS testdata_object_key,
        version.sha256 AS testdata_sha256
    FROM team_accounts account
    JOIN teams team ON team.id = account.team_id AND team.deleted_at IS NULL
    JOIN contest_teams roster ON roster.team_id = team.id
    JOIN contests contest
      ON contest.id = roster.contest_id
     AND contest.deleted_at IS NULL
     AND contest.status = 'RUNNING'
     AND contest.start_at IS NOT NULL
     AND contest.end_at IS NOT NULL
     AND now() >= contest.start_at
     AND now() < contest.end_at
    JOIN contest_problems assignment ON assignment.contest_id = contest.id
    JOIN problems problem
      ON problem.id = assignment.problem_id AND problem.deleted_at IS NULL
    JOIN problem_testdata_versions version
      ON version.problem_id = problem.id
     AND version.version = problem.testdata_version
     AND version.object_key = problem.testdata_object_key
     AND version.sha256 = problem.testdata_sha256
    WHERE account.user_id = $1 AND contest.id = $2 AND problem.id = $3
"#;

async fn load_context_pool(
    database: &PgPool,
    contest_id: i64,
    problem_id: i64,
    user_id: i64,
) -> Result<SubmissionContext, AppError> {
    sqlx::query_as::<_, SubmissionContext>(CONTEXT_QUERY)
        .bind(user_id)
        .bind(contest_id)
        .bind(problem_id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("validate submission context", error))?
        .ok_or_else(submission_not_allowed)
}

async fn load_context_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    problem_id: i64,
    user_id: i64,
) -> Result<SubmissionContext, AppError> {
    let query = format!(
        "{CONTEXT_QUERY} FOR SHARE OF account, team, roster, contest, assignment, problem, version"
    );
    sqlx::query_as::<_, SubmissionContext>(sqlx::AssertSqlSafe(query))
        .bind(user_id)
        .bind(contest_id)
        .bind(problem_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("revalidate locked submission context", error))?
        .ok_or_else(submission_not_allowed)
}

async fn enforce_rate_limit(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(team_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("lock team submission rate limit", error))?;
    let recent = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT count(*) FROM submissions
        WHERE contest_id = $1 AND team_id = $2
          AND submitted_at > now() - interval '1 minute'
        "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check submission rate limit", error))?;
    if recent >= SUBMISSION_LIMIT_PER_MINUTE {
        Err(AppError::too_many_requests(
            "SUBMISSION_RATE_LIMITED",
            "This team has submitted too many times; try again later",
        ))
    } else {
        Ok(())
    }
}

fn require_language(languages_json: &str, language: &str) -> Result<(), AppError> {
    let languages: Vec<String> = serde_json::from_str(languages_json)
        .map_err(|error| AppError::internal("parse problem language configuration", error))?;
    if languages.iter().any(|allowed| allowed == language) {
        Ok(())
    } else {
        Err(AppError::conflict(
            "LANGUAGE_NOT_ALLOWED",
            "The selected language is not enabled for this problem",
        ))
    }
}

fn submission_not_allowed() -> AppError {
    AppError::conflict(
        "SUBMISSION_NOT_ALLOWED",
        "The contest, roster, problem, or test-data state does not allow this submission",
    )
}

fn rejudge_submission_not_found() -> AppError {
    AppError::not_found("SUBMISSION_NOT_FOUND", "Submission was not found")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use project_balloon_contracts::JudgeTask;
    use sqlx::PgPool;
    use time::OffsetDateTime;

    use super::{SubmissionService, language_multiplier};
    use crate::{
        features::{
            auth::model::{AuthUser, UserType},
            scoreboard,
            submissions::model::{
                RejudgeRequest, ValidatedSubmission, ValidatedSubmissionListQuery,
                source_fingerprint,
            },
            submissions::query::SimilarityPairQuery,
        },
        object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
    };

    #[derive(Default)]
    struct MemoryStorage {
        objects: Mutex<HashMap<(String, String), Bytes>>,
    }

    #[test]
    fn p0_language_time_multipliers_are_explicit() {
        assert_eq!(language_multiplier("c"), 1.0);
        assert_eq!(language_multiplier("cpp"), 1.0);
        assert_eq!(language_multiplier("java"), 2.0);
        assert_eq!(language_multiplier("python"), 3.0);
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
            self.objects
                .lock()
                .expect("memory storage lock")
                .remove(&(bucket.to_owned(), key.to_owned()));
            Ok(())
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn submission_persists_authoritative_task_and_compensates_rejection(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('submit-team', 'test-hash', 'Submit Team', 'TEAM', true, false)
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert team user");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Submit Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert team");
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(team_id)
            .execute(&pool)
            .await
            .expect("link team account");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, end_at)
            VALUES (
                'Submission Contest', 'RUNNING', 'PRIVATE',
                now() - interval '1 hour', now() + interval '1 hour'
            )
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert running contest");
        sqlx::query(
            "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
        )
        .bind(contest_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("insert roster");
        let problem_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO problems
                (slug, title, languages, testdata_version, testdata_object_key, testdata_sha256)
            VALUES ('submit-problem', 'Submit Problem', '["cpp"]', 1,
                    'problems/1/testdata/v1/fixture.zip', $1)
            RETURNING id
            "#,
        )
        .bind("a".repeat(64))
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query(
            r#"
            INSERT INTO problem_testdata_versions
                (problem_id, version, object_key, sha256, bytes, case_count)
            VALUES ($1, 1, 'problems/1/testdata/v1/fixture.zip', $2, 100, 1)
            "#,
        )
        .bind(problem_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await
        .expect("insert test-data version");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
        let actor = AuthUser {
            id: user_id,
            username: "submit-team".into(),
            display_name: "Submit Team".into(),
            user_type: UserType::Team,
            permissions: vec![],
            password_reset_required: false,
        };
        let memory = Arc::new(MemoryStorage::default());
        let storage = ObjectStorageHandle::with_buckets(
            memory.clone(),
            "problems-test".into(),
            "sources-test".into(),
        );
        let service = SubmissionService::new(pool.clone());
        let response = service
            .submit(
                contest_id,
                ValidatedSubmission {
                    problem_id,
                    language: "cpp".into(),
                    extension: ".cpp",
                    source: Bytes::from_static(b"int main() { return 0; }"),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("create submission");
        assert_eq!(response.status, "PENDING");
        let (source_key, source_hash, fingerprint, payload) =
            sqlx::query_as::<_, (String, String, String, String)>(
                r#"
            SELECT submission.source_object_key, submission.source_sha256,
                   submission.source_fingerprint, outbox.payload
            FROM submissions submission
            JOIN submission_outbox outbox ON outbox.submission_id = submission.id
            WHERE submission.id = $1
            "#,
            )
            .bind(response.submission_id)
            .fetch_one(&pool)
            .await
            .expect("load submission and outbox");
        let task: JudgeTask = serde_json::from_str(&payload).expect("deserialize judge task");
        task.validate().expect("valid judge task");
        assert_eq!(task.judgement_id, response.judgement_id);
        assert_eq!(task.source_object_key, source_key);
        assert_eq!(task.source_sha256, source_hash);
        assert_eq!(fingerprint, source_fingerprint(b"int main() { return 0; }"));
        assert_eq!(task.testdata_version, 1);
        assert_eq!(task.testdata_sha256, "a".repeat(64));
        assert!(
            memory
                .get("sources-test", &source_key)
                .await
                .expect("stored source")
                .starts_with(b"int main")
        );
        let second_user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('submit-team-2', 'test-hash', 'Submit Team 2', 'TEAM', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert second team user");
        let second_team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Submit Team 2') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert second team");
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(second_user_id)
            .bind(second_team_id)
            .execute(&pool)
            .await
            .expect("link second team account");
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id)
            .bind(second_team_id)
            .execute(&pool)
            .await
            .expect("roster second team");
        let second_actor = AuthUser {
            id: second_user_id,
            username: "submit-team-2".into(),
            display_name: "Submit Team 2".into(),
            user_type: UserType::Team,
            permissions: vec![],
            password_reset_required: false,
        };
        let second_response = service
            .submit(
                contest_id,
                ValidatedSubmission {
                    problem_id,
                    language: "cpp".into(),
                    extension: ".cpp",
                    source: Bytes::from_static(b"int main(){ return 1; }"),
                },
                &second_actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .expect("create similar second-team submission");
        let team_event = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM realtime_outbox
                WHERE contest_id = $1 AND team_id = $2
                  AND event_type = 'SUBMISSION_STATUS_CHANGED' AND scope = 'TEAM'
            )
            "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .fetch_one(&pool)
        .await
        .expect("check team realtime event");
        assert!(team_event);

        sqlx::query(
            r#"
            UPDATE judgements
            SET verdict = 'ACCEPTED', completed_at = now(), total_time_ms = 12,
                peak_memory_kb = 1024, compile_log = $2
            WHERE id = $1
            "#,
        )
        .bind(response.judgement_id)
        .bind("compiled\u{7}success")
        .execute(&pool)
        .await
        .expect("complete original judgement");
        sqlx::query(
            r#"
            INSERT INTO runs
                (judgement_id, test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail)
            VALUES ($1, 1, 'ACCEPTED', 12, 1024, 0, $2)
            "#,
        )
        .bind(response.judgement_id)
        .bind("run\u{1}ok")
        .execute(&pool)
        .await
        .expect("insert original judgement run");
        sqlx::query("UPDATE submissions SET status = 'ACCEPTED', judged_at = now() WHERE id = $1")
            .bind(response.submission_id)
            .execute(&pool)
            .await
            .expect("complete original submission");
        let mut projection = pool.begin().await.expect("begin accepted projection");
        scoreboard::rebuild_cell(&mut projection, contest_id, team_id, problem_id)
            .await
            .expect("project accepted submission");
        projection.commit().await.expect("commit accepted projection");
        let admin_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('rejudge-root', 'test-hash', 'Rejudge Root', 'SUPER_ADMIN')
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert rejudge administrator");
        let admin = AuthUser {
            id: admin_id,
            username: "rejudge-root".into(),
            display_name: "Rejudge Root".into(),
            user_type: UserType::SuperAdmin,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        sqlx::query(
            "UPDATE submissions SET source_fingerprint = NULL, source_simhash = NULL, source_token_count = NULL WHERE id = $1",
        )
        .bind(response.submission_id)
        .execute(&pool)
        .await
        .expect("simulate a pre-similarity submission");
        let backfill = service
            .backfill_similarity(contest_id, &admin, &storage)
            .await
            .expect("backfill historical source similarity");
        assert_eq!((backfill.scanned, backfill.updated, backfill.failed), (1, 1, 0));
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT source_simhash IS NOT NULL AND source_token_count > 0 FROM submissions WHERE id = $1",
            )
            .bind(response.submission_id)
            .fetch_one(&pool)
            .await
            .expect("load backfilled similarity")
        );
        let similar_pairs = service
            .list_similarity_pairs(
                contest_id,
                &admin,
                SimilarityPairQuery {
                    problem_id: Some(problem_id),
                    language: Some("cpp".into()),
                    min_similarity_percent: 85,
                },
            )
            .await
            .expect("list approximate submission pairs");
        assert!(similar_pairs.iter().any(|pair| {
            pair.submission_id == response.submission_id
                && pair.other_submission_id == second_response.submission_id
        }));
        sqlx::query("UPDATE submissions SET status = 'CANCELLED' WHERE id = $1")
            .bind(second_response.submission_id)
            .execute(&pool)
            .await
            .expect("remove similarity fixture from pending assertions");
        sqlx::query("UPDATE submission_outbox SET status = 'CANCELLED' WHERE submission_id = $1")
            .bind(second_response.submission_id)
            .execute(&pool)
            .await
            .expect("cancel similarity fixture task");
        let expected_judgement_id = response.judgement_id;
        let (first_rejudge, concurrent_rejudge) = tokio::join!(
            service.rejudge(
                contest_id,
                response.submission_id,
                RejudgeRequest { expected_judgement_id },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            ),
            service.rejudge(
                contest_id,
                response.submission_id,
                RejudgeRequest { expected_judgement_id },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
        );
        let rejudged = match (first_rejudge, concurrent_rejudge) {
            (Ok(response), Err(_)) | (Err(_), Ok(response)) => response,
            (Ok(_), Ok(_)) => panic!("concurrent rejudge must create exactly one task"),
            (Err(first), Err(second)) => {
                panic!("one concurrent rejudge must succeed: {first:?}; {second:?}")
            }
        };
        assert_eq!(rejudged.previous_judgement_id, expected_judgement_id);
        assert_eq!(rejudged.status, "PENDING");
        let rejudge_state = sqlx::query_as::<
            _,
            (bool, Option<bool>, String, Option<OffsetDateTime>, String, String),
        >(
            r#"
            SELECT old_judgement.superseded,
                   old_judgement.active_marker,
                   old_outbox.status,
                   submission.judged_at,
                   submission.status,
                   new_outbox.payload
            FROM submissions submission
            JOIN judgements old_judgement ON old_judgement.id = $2
            JOIN submission_outbox old_outbox ON old_outbox.judgement_id = old_judgement.id
            JOIN submission_outbox new_outbox ON new_outbox.judgement_id = $3
            WHERE submission.id = $1
            "#,
        )
        .bind(response.submission_id)
        .bind(expected_judgement_id)
        .bind(rejudged.judgement_id)
        .fetch_one(&pool)
        .await
        .expect("load committed rejudge state");
        assert!(rejudge_state.0);
        assert_eq!(rejudge_state.1, None);
        assert_eq!(rejudge_state.2, "CANCELLED");
        assert_eq!(rejudge_state.3, None);
        assert_eq!(rejudge_state.4, "PENDING");
        let rejudge_task: JudgeTask =
            serde_json::from_str(&rejudge_state.5).expect("decode rejudge task");
        rejudge_task.validate().expect("valid rejudge task");
        assert_eq!(rejudge_task.judgement_id, rejudged.judgement_id);
        assert_eq!(rejudge_task.source_object_key, source_key);
        assert_eq!(rejudge_task.source_sha256, source_hash);
        let rejudge_effects = sqlx::query_as::<_, (bool, i64, i64)>(
            r#"
            SELECT cell.solved,
                   (SELECT count(*) FROM realtime_outbox
                    WHERE contest_id = $1 AND event_type = 'SUBMISSION_REJUDGED'
                      AND payload_json ->> 'submissionId' = $2::text),
                   (SELECT count(*) FROM audit_logs
                    WHERE actor_user_id = $3 AND action = 'SUBMISSION_REJUDGED'
                      AND target_id = $2::text)
            FROM contest_scoreboard_cells cell
            WHERE cell.contest_id = $1 AND cell.team_id = $4 AND cell.problem_id = $5
            "#,
        )
        .bind(contest_id)
        .bind(response.submission_id)
        .bind(admin_id)
        .bind(team_id)
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("load rejudge side effects");
        assert_eq!(rejudge_effects, (false, 2, 1));

        let all_submissions = || ValidatedSubmissionListQuery {
            team_id: None,
            problem_id: None,
            status: None,
            language: None,
            page: 0,
            size: 25,
            offset: 0,
        };
        let own_list = service
            .list_own(contest_id, &actor, all_submissions())
            .await
            .expect("team lists own submissions");
        assert_eq!(own_list.total_elements, 1);
        assert_eq!(own_list.content[0].active_judgement_id, Some(rejudged.judgement_id));
        let admin_list = service
            .list_admin(
                contest_id,
                &admin,
                ValidatedSubmissionListQuery {
                    status: Some("PENDING".into()),
                    ..all_submissions()
                },
            )
            .await
            .expect("administrator filters contest submissions");
        assert_eq!(admin_list.total_elements, 1);
        let own_detail = service
            .detail_own(contest_id, response.submission_id, &actor, &storage)
            .await
            .expect("team loads own submission detail");
        assert!(own_detail.source.starts_with("int main"));
        assert_eq!(own_detail.source_sha256.as_deref(), Some(source_hash.as_str()));
        assert_eq!(own_detail.judgements.len(), 2);
        let previous = own_detail
            .judgements
            .iter()
            .find(|judgement| judgement.id == expected_judgement_id)
            .expect("detail retains previous judgement");
        assert!(previous.superseded);
        assert_eq!(previous.compile_log.as_deref(), Some("compiledsuccess"));
        assert_eq!(previous.runs.len(), 1);
        assert_eq!(previous.runs[0].stderr_tail.as_deref(), Some("runok"));
        let admin_detail = service
            .detail_admin(contest_id, response.submission_id, &admin, &storage)
            .await
            .expect("administrator loads contest submission detail");
        assert_eq!(admin_detail.summary.id, response.submission_id);

        let other_user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('other-team', 'test-hash', 'Other Team', 'TEAM')
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert unrelated team user");
        let other_team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Other Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert unrelated team");
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(other_user_id)
            .bind(other_team_id)
            .execute(&pool)
            .await
            .expect("link unrelated team account");
        let other_actor = AuthUser {
            id: other_user_id,
            username: "other-team".into(),
            display_name: "Other Team".into(),
            user_type: UserType::Team,
            permissions: Vec::new(),
            password_reset_required: false,
        };
        assert!(
            service
                .detail_own(contest_id, response.submission_id, &other_actor, &storage)
                .await
                .is_err(),
            "another team must not enumerate submission details"
        );

        let queued =
            service.judge_queue_status(contest_id, &admin).await.expect("load queued judge status");
        assert!(!queued.drained);
        assert_eq!(queued.pending_submissions, 1);
        assert_eq!(queued.judging_submissions, 0);
        assert_eq!(queued.outbox_pending, 1);
        assert_eq!(queued.outbox_failed, 0);

        sqlx::query(
            "UPDATE submission_outbox SET status='SENT',sent_at=now() WHERE judgement_id=$1",
        )
        .bind(rejudged.judgement_id)
        .execute(&pool)
        .await
        .expect("mark rejudge task sent");
        sqlx::query("UPDATE submissions SET status='JUDGING' WHERE id=$1")
            .bind(response.submission_id)
            .execute(&pool)
            .await
            .expect("mark rejudged submission in flight");
        let judging = service
            .judge_queue_status(contest_id, &admin)
            .await
            .expect("load in-flight judge status");
        assert!(!judging.drained);
        assert_eq!(judging.pending_submissions, 0);
        assert_eq!(judging.judging_submissions, 1);
        assert_eq!(judging.outbox_pending, 0);

        sqlx::query("UPDATE judgements SET verdict='ACCEPTED',completed_at=now() WHERE id=$1")
            .bind(rejudged.judgement_id)
            .execute(&pool)
            .await
            .expect("complete rejudgement");
        sqlx::query("UPDATE submissions SET status='ACCEPTED',judged_at=now() WHERE id=$1")
            .bind(response.submission_id)
            .execute(&pool)
            .await
            .expect("complete rejudged submission");
        assert!(
            service
                .judge_queue_status(contest_id, &admin)
                .await
                .expect("load drained judge status")
                .drained
        );

        sqlx::query(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, status)
            SELECT $1, $2, $3, 'cpp', 'fixture/rate-' || value, 1, 'PENDING'
            FROM generate_series(1, 19) value
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("fill exact rolling rate limit");
        let before = memory.objects.lock().expect("memory storage lock").len();
        let rejected = service
            .submit(
                contest_id,
                ValidatedSubmission {
                    problem_id,
                    language: "cpp".into(),
                    extension: ".cpp",
                    source: Bytes::from_static(b"int main() { return 1; }"),
                },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await;
        assert!(rejected.is_err());
        assert_eq!(memory.objects.lock().expect("memory storage lock").len(), before);
    }
}
