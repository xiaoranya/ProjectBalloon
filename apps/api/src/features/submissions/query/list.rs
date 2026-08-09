use sqlx::PgPool;

use crate::{error::AppError, features::auth::model::AuthUser, pagination::PageResponse};

use super::super::model::{
    JudgeQueueStatusResponse, SubmissionSummary, ValidatedSubmissionListQuery,
};
use super::super::service::SubmissionService;
use super::{require_admin_access, require_team_account, submission_not_found, team_id_for_user};

impl SubmissionService {
    pub async fn list_own(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: ValidatedSubmissionListQuery,
    ) -> Result<PageResponse<SubmissionSummary>, AppError> {
        require_team_account(actor)?;
        let team_id = team_id_for_user(&self.database, actor.id).await?;
        let mut page = list(&self.database, contest_id, Some(team_id), query).await?;
        // Match the detail endpoint: teams must not see verdicts, timing, or
        // scores while the contest is live and feedback is restricted. The
        // list endpoint previously leaked these fields past the feedback
        // policy configured on the contest.
        let policy = sqlx::query_as::<_, (String, String)>(
            "SELECT feedback_policy, status FROM contests WHERE id = $1",
        )
        .bind(contest_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load submission feedback policy", error))?;
        if let Some((feedback_policy, status)) = policy
            && !matches!(status.as_str(), "ENDED" | "ARCHIVED")
        {
            for summary in &mut page.content {
                mask_summary_feedback(summary, &feedback_policy);
            }
        }
        Ok(page)
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
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| AppError::internal("check submission contest", error))?;
    if !active {
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
               judgement.peak_memory_kb,
               judgement.score_milli
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

fn mask_summary_feedback(summary: &mut SubmissionSummary, policy: &str) {
    if policy == "FULL" {
        return;
    }
    summary.verdict = None;
    summary.total_time_ms = None;
    summary.peak_memory_kb = None;
    if policy == "NONE" {
        summary.score_milli = None;
    }
}
