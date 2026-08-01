use std::collections::HashMap;

use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    object_storage::ObjectStorageHandle,
    pagination::PageResponse,
};

use super::{
    model::{
        JudgeQueueStatusResponse, JudgementDetail, JudgementSubtaskScore, RunDetail,
        SubmissionDetail, SubmissionSummary, ValidatedSubmissionListQuery,
    },
    service::SubmissionService,
};

#[derive(Debug, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityGroupResponse {
    pub problem_id: i64,
    pub language: String,
    pub fingerprint: String,
    pub submission_ids: Vec<i64>,
    pub team_ids: Vec<i64>,
    pub submission_count: i64,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityPairResponse {
    pub problem_id: i64,
    pub language: String,
    pub submission_id: i64,
    pub team_id: i64,
    pub other_submission_id: i64,
    pub other_team_id: i64,
    pub hamming_distance: i32,
    pub similarity_percent: i32,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SimilarityBackfillResponse {
    pub scanned: i64,
    pub updated: i64,
    pub failed: i64,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimilarityQuery {
    pub problem_id: Option<i64>,
    pub language: Option<String>,
    #[serde(default = "default_similarity_group_size")]
    pub min_group_size: u32,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimilarityPairQuery {
    pub problem_id: Option<i64>,
    pub language: Option<String>,
    #[serde(default = "default_similarity_percent")]
    pub min_similarity_percent: u32,
}

const fn default_similarity_group_size() -> u32 {
    2
}

const fn default_similarity_percent() -> u32 {
    85
}

impl SimilarityQuery {
    fn validate(self) -> Result<(Option<i64>, Option<String>, i64), AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if !(2..=100).contains(&self.min_group_size) {
            return Err(AppError::validation(
                "minGroupSize",
                "must contain a value between 2 and 100",
            ));
        }
        let language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if language
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(AppError::validation("language", "must be c, cpp, java, or python"));
        }
        Ok((
            self.problem_id,
            language.filter(|value| !value.is_empty()),
            i64::from(self.min_group_size),
        ))
    }
}

impl SimilarityPairQuery {
    fn validate(self) -> Result<(Option<i64>, Option<String>, i32), AppError> {
        if self.problem_id.is_some_and(|id| id <= 0) {
            return Err(AppError::validation("problemId", "must be positive"));
        }
        if !(50..=100).contains(&self.min_similarity_percent) {
            return Err(AppError::validation(
                "minSimilarityPercent",
                "must contain a value between 50 and 100",
            ));
        }
        let language = self.language.map(|value| value.trim().to_ascii_lowercase());
        if language
            .as_ref()
            .is_some_and(|value| !matches!(value.as_str(), "c" | "cpp" | "java" | "python"))
        {
            return Err(AppError::validation("language", "must be c, cpp, java, or python"));
        }
        let min_similarity_percent = i32::try_from(self.min_similarity_percent).unwrap_or(100);
        Ok((self.problem_id, language.filter(|value| !value.is_empty()), min_similarity_percent))
    }
}

impl SubmissionService {
    pub async fn backfill_similarity(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        storage: &ObjectStorageHandle,
    ) -> Result<SimilarityBackfillResponse, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let candidates = sqlx::query_as::<_, (i64, String, String)>(
            r#"
            SELECT id, source_object_key, source_sha256
            FROM submissions
            WHERE contest_id = $1 AND source_simhash IS NULL AND source_sha256 IS NOT NULL
            ORDER BY id
            LIMIT 1000
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load submissions for similarity backfill", error))?;
        let scanned = i64::try_from(candidates.len()).unwrap_or(i64::MAX);
        let mut updated = 0_i64;
        let mut failed = 0_i64;
        for (submission_id, object_key, expected_hash) in candidates {
            let source = match storage.backend().get(storage.source_bucket(), &object_key).await {
                Ok(source) if hex::encode(sha2::Sha256::digest(&source)) == expected_hash => source,
                _ => {
                    failed += 1;
                    continue;
                }
            };
            let signature = super::model::source_similarity_signature(&source);
            let fingerprint = super::model::source_fingerprint(&source);
            let changed = sqlx::query(
                "UPDATE submissions SET source_fingerprint = $2, source_simhash = $3, source_token_count = $4 WHERE id = $1 AND source_simhash IS NULL",
            )
            .bind(submission_id)
            .bind(fingerprint)
            .bind(signature.simhash)
            .bind(signature.token_count)
            .execute(&self.database)
            .await
            .map_err(|error| AppError::internal("persist similarity backfill", error))?
            .rows_affected();
            updated += i64::try_from(changed).unwrap_or(i64::MAX);
        }
        Ok(SimilarityBackfillResponse { scanned, updated, failed })
    }

    pub async fn list_similarity(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: SimilarityQuery,
    ) -> Result<Vec<SimilarityGroupResponse>, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let (problem_id, language, min_group_size) = query.validate()?;
        sqlx::query_as::<_, SimilarityGroupResponse>(
            r#"
            SELECT problem_id, language, source_fingerprint AS fingerprint,
                   array_agg(id ORDER BY submitted_at, id) AS submission_ids,
                   array_agg(team_id ORDER BY submitted_at, id) AS team_ids,
                   count(*) AS submission_count
            FROM submissions
            WHERE contest_id = $1 AND source_fingerprint IS NOT NULL
              AND ($2::bigint IS NULL OR problem_id = $2)
              AND ($3::text IS NULL OR language = $3)
            GROUP BY problem_id, language, source_fingerprint
            HAVING count(*) >= $4
            ORDER BY count(*) DESC, min(submitted_at), problem_id, language, source_fingerprint
            LIMIT 500
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(language)
        .bind(min_group_size)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list submission similarity groups", error))
    }

    pub async fn list_similarity_pairs(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: SimilarityPairQuery,
    ) -> Result<Vec<SimilarityPairResponse>, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let (problem_id, language, min_similarity_percent) = query.validate()?;
        sqlx::query_as::<_, SimilarityPairResponse>(
            r#"
            WITH pairs AS (
                SELECT a.problem_id, a.language,
                       a.id AS submission_id, a.team_id,
                       b.id AS other_submission_id, b.team_id AS other_team_id,
                       bit_count((a.source_simhash # b.source_simhash)::bit(64))::int AS hamming_distance
                FROM submissions a
                JOIN submissions b
                  ON b.contest_id = a.contest_id
                 AND b.problem_id = a.problem_id
                 AND b.language = a.language
                 AND b.id > a.id
                 AND b.team_id <> a.team_id
                WHERE a.contest_id = $1
                  AND a.source_simhash IS NOT NULL AND b.source_simhash IS NOT NULL
                  AND ($2::bigint IS NULL OR a.problem_id = $2)
                  AND ($3::text IS NULL OR a.language = $3)
            )
            SELECT problem_id, language, submission_id, team_id,
                   other_submission_id, other_team_id, hamming_distance,
                   round((100.0 * (64 - hamming_distance) / 64.0))::int AS similarity_percent
            FROM pairs
            WHERE round((100.0 * (64 - hamming_distance) / 64.0)) >= $4
            ORDER BY hamming_distance, problem_id, language, submission_id, other_submission_id
            LIMIT 1000
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(language)
        .bind(min_similarity_percent)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list submission similarity pairs", error))
    }

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
    let source = if summary.language == "output" {
        "[Output-only ZIP archive]".to_owned()
    } else {
        String::from_utf8(source.to_vec()).map_err(|_| {
            AppError::conflict(
                "SUBMISSION_SOURCE_INVALID",
                "Stored submission source is not valid UTF-8",
            )
        })?
    };
    let mut judgements = sqlx::query_as::<_, JudgementDetail>(
        r#"
        SELECT id, verdict, total_time_ms, peak_memory_kb, compile_log, worker_id,
               started_at, completed_at, created_at, version, superseded,
               active_marker IS TRUE AS active, score_milli
        FROM judgements
        WHERE submission_id = $1
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(submission_id)
    .fetch_all(database)
    .await
    .map_err(|error| AppError::internal("load submission judgements", error))?;
    let judgement_ids: Vec<Uuid> = judgements.iter().map(|judgement| judgement.id).collect();
    let mut runs_by_judgement = HashMap::<Uuid, Vec<RunDetail>>::new();
    let mut scores_by_judgement = HashMap::<Uuid, Vec<JudgementSubtaskScore>>::new();
    if !judgement_ids.is_empty() {
        let runs = sqlx::query_as::<_, (Uuid, i32, Option<String>, Option<i32>, Option<i32>, Option<i32>, Option<String>)>(
            "SELECT judgement_id, test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail FROM runs WHERE judgement_id = ANY($1) ORDER BY judgement_id, test_index",
        )
        .bind(&judgement_ids)
        .fetch_all(database)
        .await
        .map_err(|error| AppError::internal("load submission runs", error))?;
        for (judgement_id, test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail) in runs
        {
            runs_by_judgement.entry(judgement_id).or_default().push(RunDetail {
                test_index,
                verdict,
                time_ms,
                memory_kb,
                exit_code,
                stderr_tail,
            });
        }
        let scores = sqlx::query_as::<_, (Uuid, String, String, i32, i32, i32, i32)>(
            r#"SELECT score.judgement_id, subtask.subtask_key, subtask.name,
                      score.score_milli, subtask.score_milli AS max_score_milli,
                      score.passed_tests, score.total_tests
               FROM judgement_subtask_scores score
               JOIN contest_problem_subtasks subtask ON subtask.id=score.subtask_id
               WHERE score.judgement_id = ANY($1)
               ORDER BY score.judgement_id, subtask.display_order"#,
        )
        .bind(&judgement_ids)
        .fetch_all(database)
        .await
        .map_err(|error| AppError::internal("load submission subtask scores", error))?;
        for (
            judgement_id,
            subtask_key,
            name,
            score_milli,
            max_score_milli,
            passed_tests,
            total_tests,
        ) in scores
        {
            scores_by_judgement.entry(judgement_id).or_default().push(JudgementSubtaskScore {
                subtask_key,
                name,
                score_milli,
                max_score_milli,
                passed_tests,
                total_tests,
            });
        }
    }
    for judgement in &mut judgements {
        judgement.compile_log = safe_text(judgement.compile_log.take(), 65_536);
        let mut runs = runs_by_judgement.remove(&judgement.id).unwrap_or_default();
        for run in &mut runs {
            run.stderr_tail = safe_text(run.stderr_tail.take(), 8_192);
        }
        judgement.runs = runs;
        judgement.subtask_scores = scores_by_judgement.remove(&judgement.id).unwrap_or_default();
    }
    let mut detail = SubmissionDetail { summary, source, source_sha256, judgements };
    if required_team_id.is_some() {
        let (feedback_policy, status) = sqlx::query_as::<_, (String, String)>(
            "SELECT feedback_policy,status FROM contests WHERE id=$1",
        )
        .bind(contest_id)
        .fetch_one(database)
        .await
        .map_err(|error| AppError::internal("load submission feedback policy", error))?;
        if !matches!(status.as_str(), "ENDED" | "ARCHIVED") {
            apply_feedback_policy(&mut detail, &feedback_policy);
        }
    }
    Ok(detail)
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

fn apply_feedback_policy(detail: &mut SubmissionDetail, policy: &str) {
    if policy == "FULL" {
        return;
    }
    detail.summary.verdict = None;
    detail.summary.total_time_ms = None;
    detail.summary.peak_memory_kb = None;
    if policy == "NONE" {
        detail.summary.score_milli = None;
    }
    for judgement in &mut detail.judgements {
        judgement.verdict = None;
        judgement.total_time_ms = None;
        judgement.peak_memory_kb = None;
        judgement.compile_log = None;
        judgement.worker_id = None;
        judgement.runs.clear();
        judgement.subtask_scores.clear();
        if policy == "NONE" {
            judgement.score_milli = None;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{SimilarityPairQuery, SimilarityQuery};

    #[test]
    fn similarity_filters_are_bounded_and_normalized() {
        let (problem_id, language, minimum) = SimilarityQuery {
            problem_id: Some(7),
            language: Some(" Cpp ".to_owned()),
            min_group_size: 3,
        }
        .validate()
        .expect("valid similarity filters");
        assert_eq!(problem_id, Some(7));
        assert_eq!(language.as_deref(), Some("cpp"));
        assert_eq!(minimum, 3);
        assert!(
            SimilarityQuery { problem_id: None, language: None, min_group_size: 1 }
                .validate()
                .is_err()
        );
    }

    #[test]
    fn similarity_pair_threshold_matches_displayed_percentage() {
        let (_, _, min_similarity_percent) = SimilarityPairQuery {
            problem_id: None,
            language: Some("CPP".to_owned()),
            min_similarity_percent: 85,
        }
        .validate()
        .expect("valid pair filters");
        assert_eq!(min_similarity_percent, 85);
        assert!(
            SimilarityPairQuery { problem_id: None, language: None, min_similarity_percent: 49 }
                .validate()
                .is_err()
        );
    }
}
