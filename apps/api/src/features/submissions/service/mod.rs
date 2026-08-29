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
}

impl SubmissionService {
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
        invalid => Err(AppError::internal_message("invalid problems.judge_mode", invalid)),
    }
}

const SUBMISSION_CONTEXT_SQL: &str = r#"
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
    sqlx::query_as::<_, SubmissionContext>(SUBMISSION_CONTEXT_SQL)
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
        "{SUBMISSION_CONTEXT_SQL} FOR SHARE OF account, team, roster, contest, assignment, problem, version"
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

mod rejudge;
#[cfg(test)]
mod tests;
