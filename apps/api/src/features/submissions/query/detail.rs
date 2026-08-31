use std::collections::HashMap;

use sha2::Digest;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError, features::auth::model::AuthUser, object_storage::ObjectStorageHandle,
};

use crate::features::submissions::model::{
    JudgementDetail, JudgementSubtaskScore, RunDetail, SubmissionDetail, SubmissionSummary,
};
use crate::features::submissions::query::{
    require_admin_access, require_team_account, submission_not_found, team_id_for_user,
};
use crate::features::submissions::service::SubmissionService;

impl SubmissionService {
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
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(database)
    .await
    .map_err(|error| {
        AppError::internal("check submission contest", error)
            .with_contest_id(contest_id)
            .with_submission_id(submission_id)
    })?;
    if !active {
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
    .map_err(|error| {
        AppError::internal("load submission detail", error)
            .with_contest_id(contest_id)
            .with_submission_id(submission_id)
    })?
    .ok_or_else(submission_not_found)?;
    let (source_object_key, source_sha256, source_size_bytes) = sqlx::query_as::<
        _,
        (String, Option<String>, i32),
    >(
        "SELECT source_object_key, source_sha256, source_size_bytes FROM submissions WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(database)
    .await
    .map_err(|error| {
        AppError::internal("load submission source metadata", error)
            .with_submission_id(submission_id)
    })?;
    let expected_source_size = usize::try_from(source_size_bytes).unwrap_or(0);
    if expected_source_size == 0 || expected_source_size > super::super::model::MAX_SOURCE_BYTES {
        return Err(AppError::conflict(
            "SUBMISSION_SOURCE_SIZE_MISMATCH",
            "Stored submission source has an unsupported recorded size",
        ));
    }
    let source = storage
        .backend()
        .get_limited(storage.source_bucket(), &source_object_key, expected_source_size)
        .await
        .map_err(|error| {
            AppError::internal("download submission source", error)
                .with_submission_id(submission_id)
        })?;
    if source.len() != expected_source_size {
        return Err(AppError::conflict(
            "SUBMISSION_SOURCE_SIZE_MISMATCH",
            "Stored submission source does not match its recorded size",
        ));
    }
    if let Some(expected_hash) = source_sha256.as_deref()
        && hex::encode(sha2::Sha256::digest(&source)) != expected_hash
    {
        return Err(AppError::conflict(
            "SUBMISSION_SOURCE_HASH_MISMATCH",
            "Stored submission source failed integrity verification",
        ));
    }
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
    .map_err(|error| {
        AppError::internal("load submission judgements", error).with_submission_id(submission_id)
    })?;
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
        .map_err(|error| {
            AppError::internal("load submission runs", error).with_submission_id(submission_id)
        })?;
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
        .map_err(|error| {
            AppError::internal("load submission subtask scores", error)
                .with_submission_id(submission_id)
        })?;
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
        .map_err(|error| {
            AppError::internal("load submission feedback policy", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
        })?;
        if !matches!(status.as_str(), "ENDED" | "ARCHIVED") {
            apply_feedback_policy(&mut detail, &feedback_policy);
        }
    }
    Ok(detail)
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

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use uuid::Uuid;

    use super::apply_feedback_policy;
    use crate::features::submissions::model::{
        JudgementDetail, JudgementSubtaskScore, RunDetail, SubmissionDetail,
    };
    use crate::features::submissions::query::restricted_submission_summary;

    fn restricted_detail() -> SubmissionDetail {
        SubmissionDetail {
            summary: restricted_submission_summary(),
            source: "int main() {}".to_owned(),
            source_sha256: Some("hash".to_owned()),
            judgements: vec![JudgementDetail {
                id: Uuid::from_u128(1),
                verdict: Some("WRONG_ANSWER".to_owned()),
                total_time_ms: Some(12),
                peak_memory_kb: Some(2048),
                compile_log: Some("cc: error".to_owned()),
                worker_id: Some("worker-1".to_owned()),
                started_at: None,
                completed_at: None,
                created_at: OffsetDateTime::from_unix_timestamp(0).expect("epoch"),
                version: 1,
                superseded: false,
                active: true,
                score_milli: Some(100_000),
                runs: vec![RunDetail {
                    test_index: 1,
                    verdict: Some("WRONG_ANSWER".to_owned()),
                    time_ms: Some(12),
                    memory_kb: Some(2048),
                    exit_code: Some(1),
                    stderr_tail: Some("assert failed".to_owned()),
                }],
                subtask_scores: vec![JudgementSubtaskScore {
                    subtask_key: "full".to_owned(),
                    name: "Full".to_owned(),
                    score_milli: 100_000,
                    max_score_milli: 100_000,
                    passed_tests: 0,
                    total_tests: 1,
                }],
            }],
        }
    }

    #[test]
    fn full_feedback_keeps_every_judgement_field() {
        let mut detail = restricted_detail();
        apply_feedback_policy(&mut detail, "FULL");
        assert_eq!(detail.summary.verdict.as_deref(), Some("WRONG_ANSWER"));
        let judgement = &detail.judgements[0];
        assert_eq!(judgement.compile_log.as_deref(), Some("cc: error"));
        assert_eq!(judgement.worker_id.as_deref(), Some("worker-1"));
        assert_eq!(judgement.runs.len(), 1);
        assert_eq!(judgement.subtask_scores.len(), 1);
        assert_eq!(judgement.score_milli, Some(100_000));
    }

    #[test]
    fn score_feedback_clears_judgement_diagnostics_but_keeps_scores() {
        let mut detail = restricted_detail();
        apply_feedback_policy(&mut detail, "SCORE");
        assert_eq!(detail.summary.verdict, None);
        let judgement = &detail.judgements[0];
        assert_eq!(judgement.verdict, None);
        assert_eq!(judgement.compile_log, None);
        assert_eq!(judgement.worker_id, None);
        assert!(judgement.runs.is_empty());
        assert!(judgement.subtask_scores.is_empty());
        assert_eq!(judgement.score_milli, Some(100_000));
    }

    #[test]
    fn none_feedback_clears_judgement_scores_too() {
        let mut detail = restricted_detail();
        apply_feedback_policy(&mut detail, "NONE");
        assert_eq!(detail.summary.score_milli, None);
        assert_eq!(detail.judgements[0].score_milli, None);
    }
}
