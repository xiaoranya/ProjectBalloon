use sqlx::PgPool;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    object_storage::ObjectStorageHandle,
    pagination::PageResponse,
};

use super::{
    model::{
        JudgeQueueStatusResponse, JudgementDetail, RunDetail, SubmissionDetail, SubmissionSummary,
        ValidatedSubmissionListQuery,
    },
    service::SubmissionService,
};

impl SubmissionService {
    pub async fn list_own(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: ValidatedSubmissionListQuery,
    ) -> Result<PageResponse<SubmissionSummary>, AppError> {
        require_team_account(actor)?;
        let team_id = team_id_for_user(&self.database, actor.id).await?;
        list(&self.database, contest_id, Some(team_id), query).await
    }

    pub async fn list_admin(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: ValidatedSubmissionListQuery,
    ) -> Result<PageResponse<SubmissionSummary>, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        list(&self.database, contest_id, None, query).await
    }

    pub async fn judge_queue_status(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<JudgeQueueStatusResponse, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let counts = sqlx::query_as::<_, (i64, i64, i64, i64, time::OffsetDateTime)>(
            r#"
            SELECT
                (SELECT count(*) FROM submissions
                 WHERE contest_id=$1 AND status='PENDING'),
                (SELECT count(*) FROM submissions
                 WHERE contest_id=$1 AND status='JUDGING'),
                (SELECT count(*) FROM submission_outbox outbox
                 JOIN submissions submission ON submission.id=outbox.submission_id
                 WHERE submission.contest_id=$1 AND outbox.status IN ('PENDING','PUBLISHING')),
                (SELECT count(*) FROM submission_outbox outbox
                 JOIN submissions submission ON submission.id=outbox.submission_id
                 WHERE submission.contest_id=$1 AND outbox.status='FAILED'),
                now()
            FROM contests contest
            WHERE contest.id=$1 AND contest.deleted_at IS NULL
            "#,
        )
        .bind(contest_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load contest judge queue status", error))?
        .ok_or_else(submission_not_found)?;
        Ok(JudgeQueueStatusResponse {
            contest_id,
            drained: counts.0 == 0 && counts.1 == 0 && counts.2 == 0 && counts.3 == 0,
            pending_submissions: counts.0,
            judging_submissions: counts.1,
            outbox_pending: counts.2,
            outbox_failed: counts.3,
            checked_at: counts.4,
        })
    }

    pub async fn detail_own(
        &self,
        contest_id: i64,
        submission_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<SubmissionDetail, AppError> {
        require_team_account(actor)?;
        let team_id = team_id_for_user(&self.database, actor.id).await?;
        detail(&self.database, contest_id, submission_id, Some(team_id), storage).await
    }

    pub async fn detail_admin(
        &self,
        contest_id: i64,
        submission_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<SubmissionDetail, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        detail(&self.database, contest_id, submission_id, None, storage).await
    }
}

fn require_team_account(actor: &AuthUser) -> Result<(), AppError> {
    if actor.user_type == UserType::Team {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "TEAM_ACCOUNT_REQUIRED",
            "Only a team account can view team submissions",
        ))
    }
}

async fn team_id_for_user(database: &PgPool, user_id: i64) -> Result<i64, AppError> {
    sqlx::query_scalar("SELECT team_id FROM team_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(database)
        .await
        .map_err(|error| AppError::internal("load submission team identity", error))?
        .ok_or_else(submission_not_found)
}

pub(super) async fn require_admin_access(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if contest_id <= 0 {
        return Err(submission_not_found());
    }
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id = $1 AND user_id = $2)",
    )
    .bind(contest_id)
    .bind(actor.id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check submission administrator scope", error))?;
    if assigned { Ok(()) } else { Err(submission_not_found()) }
}

async fn list(
    database: &PgPool,
    contest_id: i64,
    required_team_id: Option<i64>,
    query: ValidatedSubmissionListQuery,
) -> Result<PageResponse<SubmissionSummary>, AppError> {
    if contest_id <= 0 {
        return Err(submission_not_found());
    }
    let total_elements = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT count(*)
        FROM submissions submission
        WHERE submission.contest_id = $1
          AND ($2::bigint IS NULL OR submission.team_id = $2)
          AND ($3::bigint IS NULL OR submission.team_id = $3)
          AND ($4::bigint IS NULL OR submission.problem_id = $4)
          AND ($5::text IS NULL OR submission.status = $5)
          AND ($6::text IS NULL OR submission.language = $6)
        "#,
    )
    .bind(contest_id)
    .bind(required_team_id)
    .bind(query.team_id)
    .bind(query.problem_id)
    .bind(query.status.as_deref())
    .bind(query.language.as_deref())
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("count submissions", error))?;
    let content = sqlx::query_as::<_, SubmissionSummary>(
        r#"
        SELECT submission.id,
               submission.contest_id,
               submission.problem_id,
               assignment.alias AS problem_alias,
               submission.team_id,
               team.name AS team_name,
               submission.language,
               submission.source_size_bytes,
               submission.status,
               submission.submitted_at,
               submission.judged_at,
               judgement.id AS active_judgement_id,
               judgement.verdict,
               judgement.total_time_ms,
               judgement.peak_memory_kb
        FROM submissions submission
        JOIN teams team ON team.id = submission.team_id
        JOIN contest_problems assignment
          ON assignment.contest_id = submission.contest_id
         AND assignment.problem_id = submission.problem_id
        LEFT JOIN judgements judgement
          ON judgement.submission_id = submission.id
         AND judgement.active_marker IS TRUE
        WHERE submission.contest_id = $1
          AND ($2::bigint IS NULL OR submission.team_id = $2)
          AND ($3::bigint IS NULL OR submission.team_id = $3)
          AND ($4::bigint IS NULL OR submission.problem_id = $4)
          AND ($5::text IS NULL OR submission.status = $5)
          AND ($6::text IS NULL OR submission.language = $6)
        ORDER BY submission.submitted_at DESC, submission.id DESC
        LIMIT $7 OFFSET $8
        "#,
    )
    .bind(contest_id)
    .bind(required_team_id)
    .bind(query.team_id)
    .bind(query.problem_id)
    .bind(query.status.as_deref())
    .bind(query.language.as_deref())
    .bind(i64::from(query.size))
    .bind(query.offset)
    .fetch_all(database)
    .await
    .map_err(|error| AppError::internal("list submissions", error))?;
    Ok(PageResponse::new(content, query.page, query.size, total_elements))
}

async fn detail(
    database: &PgPool,
    contest_id: i64,
    submission_id: i64,
    required_team_id: Option<i64>,
    storage: &ObjectStorageHandle,
) -> Result<SubmissionDetail, AppError> {
    if contest_id <= 0 || submission_id <= 0 {
        return Err(submission_not_found());
    }
    let summary = sqlx::query_as::<_, SubmissionSummary>(
        r#"
        SELECT submission.id,
               submission.contest_id,
               submission.problem_id,
               assignment.alias AS problem_alias,
               submission.team_id,
               team.name AS team_name,
               submission.language,
               submission.source_size_bytes,
               submission.status,
               submission.submitted_at,
               submission.judged_at,
               judgement.id AS active_judgement_id,
               judgement.verdict,
               judgement.total_time_ms,
               judgement.peak_memory_kb
        FROM submissions submission
        JOIN teams team ON team.id = submission.team_id
        JOIN contest_problems assignment
          ON assignment.contest_id = submission.contest_id
         AND assignment.problem_id = submission.problem_id
        LEFT JOIN judgements judgement
          ON judgement.submission_id = submission.id
         AND judgement.active_marker IS TRUE
        WHERE submission.id = $1 AND submission.contest_id = $2
          AND ($3::bigint IS NULL OR submission.team_id = $3)
        "#,
    )
    .bind(submission_id)
    .bind(contest_id)
    .bind(required_team_id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load submission detail", error))?
    .ok_or_else(submission_not_found)?;
    let (source_object_key, source_sha256) = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT source_object_key, source_sha256 FROM submissions WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("load submission source metadata", error))?;
    let source = storage
        .backend()
        .get(storage.source_bucket(), &source_object_key)
        .await
        .map_err(|error| AppError::internal("download submission source", error))?;
    let source = String::from_utf8(source.to_vec()).map_err(|_| {
        AppError::conflict(
            "SUBMISSION_SOURCE_INVALID",
            "Stored submission source is not valid UTF-8",
        )
    })?;
    let mut judgements = sqlx::query_as::<_, JudgementDetail>(
        r#"
        SELECT id, verdict, total_time_ms, peak_memory_kb, compile_log, worker_id,
               started_at, completed_at, created_at, version, superseded,
               active_marker IS TRUE AS active
        FROM judgements
        WHERE submission_id = $1
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(submission_id)
    .fetch_all(database)
    .await
    .map_err(|error| AppError::internal("load submission judgements", error))?;
    for judgement in &mut judgements {
        judgement.compile_log = safe_text(judgement.compile_log.take(), 65_536);
        let mut runs = sqlx::query_as::<_, RunDetail>(
            r#"
            SELECT test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail
            FROM runs
            WHERE judgement_id = $1
            ORDER BY test_index
            "#,
        )
        .bind(judgement.id)
        .fetch_all(database)
        .await
        .map_err(|error| AppError::internal("load judgement runs", error))?;
        for run in &mut runs {
            run.stderr_tail = safe_text(run.stderr_tail.take(), 8_192);
        }
        judgement.runs = runs;
    }
    Ok(SubmissionDetail { summary, source, source_sha256, judgements })
}

fn safe_text(value: Option<String>, limit: usize) -> Option<String> {
    value.map(|value| {
        value
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
            .take(limit)
            .collect()
    })
}

fn submission_not_found() -> AppError {
    AppError::not_found("SUBMISSION_NOT_FOUND", "Submission was not found")
}
