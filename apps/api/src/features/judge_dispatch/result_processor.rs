use project_balloon_contracts::JudgeResult;
use serde_json::json;
use sqlx::PgPool;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::features::{
    submissions::SubmissionStatus,
    {balloons, scoreboard, scoring},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResultOutcome {
    Applied,
    Duplicate,
    Superseded,
}

#[derive(Debug, Error)]
pub enum ApplyResultError {
    #[error("invalid judge result: {0}")]
    Invalid(String),
    #[error("judge result conflicts with persisted state: {0}")]
    Conflict(String),
    #[error("database error while applying judge result: {0}")]
    Database(#[from] sqlx::Error),
}

impl ApplyResultError {
    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        matches!(self, Self::Invalid(_) | Self::Conflict(_))
    }
}

#[derive(Clone)]
pub struct JudgeResultProcessor {
    database: PgPool,
    outbox: Option<crate::features::realtime::RealtimeOutbox>,
    projection: Option<scoreboard::ScoreboardProjection>,
}

#[derive(sqlx::FromRow)]
struct ResultContext {
    submission_id: i64,
    result_message_id: Option<Uuid>,
    completed: bool,
    superseded: bool,
    status: String,
    submission_scope: String,
    contest_id: Option<i64>,
    team_id: Option<i64>,
    problem_id: i64,
    participant_user_id: Option<i64>,
    training_enrollment_id: Option<i64>,
    submitted_at: OffsetDateTime,
    start_at: Option<OffsetDateTime>,
    scoring_icpc: bool,
    aggregation_best: bool,
    max_score_milli: i64,
}

impl JudgeResultProcessor {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database, outbox: None, projection: None }
    }

    #[must_use]
    pub fn with_outbox_option(
        mut self,
        outbox: Option<crate::features::realtime::RealtimeOutbox>,
    ) -> Self {
        self.outbox = outbox;
        self
    }

    #[must_use]
    pub fn with_projection_option(
        mut self,
        projection: Option<scoreboard::ScoreboardProjection>,
    ) -> Self {
        self.projection = projection;
        self
    }

    pub async fn apply(
        &self,
        result: &JudgeResult,
    ) -> Result<ApplyResultOutcome, ApplyResultError> {
        result.validate().map_err(|error| ApplyResultError::Invalid(error.to_string()))?;
        let mut transaction = self.database.begin().await?;
        let persisted = sqlx::query_as::<_, ResultContext>(
            r#"
            SELECT j.submission_id,
                   j.result_message_id,
                   j.completed_at IS NOT NULL AS completed,
                   j.superseded,
                   s.status,
                   s.submission_scope,
                   s.contest_id,
                   s.team_id,
                   s.problem_id,
                   s.participant_user_id,
                   s.training_enrollment_id,
                   s.submitted_at,
                   (SELECT c.start_at FROM contests c WHERE c.id = s.contest_id) AS start_at,
                   coalesce((SELECT c.scoring_mode = 'ICPC' FROM contests c WHERE c.id = s.contest_id), true)
                       AS scoring_icpc,
                   coalesce((SELECT c.score_aggregation = 'BEST' FROM contests c WHERE c.id = s.contest_id), true)
                       AS aggregation_best,
                   coalesce((SELECT cp.max_score_milli FROM contest_problems cp
                             WHERE cp.contest_id = s.contest_id AND cp.problem_id = s.problem_id), 0)
                       AS max_score_milli
            FROM judgements j
            JOIN submissions s ON s.id = j.submission_id
            WHERE j.id = $1
            FOR UPDATE OF j, s
            "#,
        )
        .bind(result.judgement_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(context) = persisted else {
            return Err(ApplyResultError::Conflict(format!(
                "unknown judgement {}",
                result.judgement_id
            )));
        };
        if context.submission_id != result.submission_id {
            return Err(ApplyResultError::Conflict(format!(
                "judgement belongs to submission {}, not {}",
                context.submission_id, result.submission_id
            )));
        }
        if context.superseded {
            transaction.commit().await?;
            return Ok(ApplyResultOutcome::Superseded);
        }
        if context.completed || context.result_message_id.is_some() {
            if context.result_message_id == Some(result.message_id) {
                transaction.commit().await?;
                return Ok(ApplyResultOutcome::Duplicate);
            }
            return Err(ApplyResultError::Conflict(format!(
                "judgement {} already has a different final result",
                result.judgement_id
            )));
        }

        let current = SubmissionStatus::parse(&context.status).ok_or_else(|| {
            ApplyResultError::Conflict(format!(
                "submission {} has unknown status {}",
                context.submission_id, context.status
            ))
        })?;
        if current.is_terminal() {
            return Err(ApplyResultError::Conflict(format!(
                "submission {} already ended as {}",
                context.submission_id,
                current.as_str()
            )));
        }

        sqlx::query(
            r#"
            UPDATE judgements
            SET verdict = $2,
                total_time_ms = $3,
                peak_memory_kb = $4,
                compile_log = $5,
                worker_id = $6,
                started_at = $7,
                completed_at = $8,
                result_message_id = $9,
                version = version + 1
            WHERE id = $1
            "#,
        )
        .bind(result.judgement_id)
        .bind(result.verdict.as_str())
        .bind(result.total_time_ms)
        .bind(result.peak_memory_kb)
        .bind(result.compile_log.as_deref())
        .bind(&result.worker_id)
        .bind(result.started_at)
        .bind(result.completed_at)
        .bind(result.message_id)
        .execute(&mut *transaction)
        .await?;
        if !result.runs.is_empty() {
            // One round trip for every test case instead of one per run.
            let test_indexes: Vec<i32> = result.runs.iter().map(|run| run.test_index).collect();
            let verdicts: Vec<&str> = result.runs.iter().map(|run| run.verdict.as_str()).collect();
            let time_ms: Vec<i32> = result.runs.iter().map(|run| run.time_ms).collect();
            let memory_kb: Vec<i32> = result.runs.iter().map(|run| run.memory_kb).collect();
            let exit_codes: Vec<Option<i32>> =
                result.runs.iter().map(|run| run.exit_code).collect();
            let stderr_tails: Vec<Option<&str>> =
                result.runs.iter().map(|run| run.stderr_tail.as_deref()).collect();
            sqlx::query(
                r#"
                INSERT INTO runs
                    (judgement_id, test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail)
                SELECT $1, t.test_index, t.verdict, t.time_ms, t.memory_kb, t.exit_code, t.stderr_tail
                FROM UNNEST($2::int[], $3::text[], $4::int[], $5::int[], $6::int[], $7::text[])
                    AS t(test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail)
                "#,
            )
            .bind(result.judgement_id)
            .bind(&test_indexes)
            .bind(&verdicts)
            .bind(&time_ms)
            .bind(&memory_kb)
            .bind(&exit_codes)
            .bind(&stderr_tails)
            .execute(&mut *transaction)
            .await?;
        }
        let score_milli = if context.submission_scope == "CONTEST" {
            let contest_id = context.contest_id.ok_or_else(|| {
                ApplyResultError::Conflict("contest submission has no contest".into())
            })?;
            scoring::score_judgement(
                &mut transaction,
                result.judgement_id,
                contest_id,
                context.problem_id,
                result.verdict,
                &result.runs,
            )
            .await?
        } else if context.submission_scope == "PRACTICE" {
            let score = if result.verdict.as_str() == "ACCEPTED" { 100_000 } else { 0 };
            sqlx::query("UPDATE judgements SET score_milli=$2 WHERE id=$1")
                .bind(result.judgement_id)
                .bind(score)
                .execute(&mut *transaction)
                .await?;
            score
        } else {
            return Err(ApplyResultError::Conflict(format!(
                "unknown submission scope {}",
                context.submission_scope
            )));
        };
        sqlx::query(
            r#"
            UPDATE submissions
            SET status = 'COMPLETED', verdict = $2, judged_at = $3
            WHERE id = $1
            "#,
        )
        .bind(context.submission_id)
        .bind(result.verdict.as_str())
        .bind(result.completed_at)
        .execute(&mut *transaction)
        .await?;
        if context.submission_scope == "CONTEST" {
            let contest_id = context.contest_id.ok_or_else(|| {
                ApplyResultError::Conflict("contest submission has no contest".into())
            })?;
            let team_id = context.team_id.ok_or_else(|| {
                ApplyResultError::Conflict("contest submission has no team".into())
            })?;
            // With a Redis projection the running-contest scoreboard is
            // maintained incrementally after commit; without one the DB
            // projection (full history rescan) stays authoritative.
            if self.projection.is_none() {
                scoreboard::rebuild_cell(&mut transaction, contest_id, team_id, context.problem_id)
                    .await?;
            }
            balloons::generate_for_accepted(
                self.outbox.as_ref(),
                &mut transaction,
                context.submission_id,
                contest_id,
                team_id,
                context.problem_id,
                result.verdict.as_str() == "ACCEPTED",
            )
            .await?;
            crate::features::realtime::outbox::enqueue_optional(
                self.outbox.as_ref(),
                contest_id,
                "SUBMISSION_STATUS_CHANGED",
                "TEAM",
                Some(team_id),
                json!({
                    "submissionId": context.submission_id,
                    "judgementId": result.judgement_id,
                    "status": result.verdict.as_str(),
                    "verdict": result.verdict.as_str(),
                    "totalTimeMs": result.total_time_ms,
                    "peakMemoryKb": result.peak_memory_kb
                    ,"scoreMilli": score_milli
                }),
            )
            .await
            .map_err(|error| {
                ApplyResultError::Conflict(format!("enqueue status event failed: {error:?}"))
            })?;
        } else {
            apply_practice_progress(
                &mut transaction,
                &context,
                result.verdict.as_str() == "ACCEPTED",
            )
            .await?;
        }
        transaction.commit().await?;
        self.apply_scoreboard_event(&context, result.verdict.as_str(), i64::from(score_milli))
            .await;
        Ok(ApplyResultOutcome::Applied)
    }

    /// Pushes the finished judgement into the Redis scoreboard projection.
    /// Best-effort by design: a failure only makes the cell dirty (replayed
    /// from PostgreSQL on the next read), it never fails a judged submission.
    async fn apply_scoreboard_event(
        &self,
        context: &ResultContext,
        verdict: &str,
        score_milli: i64,
    ) {
        let (Some(projection), Some(contest_id), Some(team_id), Some(start_at)) =
            (&self.projection, context.contest_id, context.team_id, context.start_at)
        else {
            return;
        };
        let event = scoreboard::ScoreEvent {
            contest_id,
            team_id,
            problem_id: context.problem_id,
            verdict: verdict.to_owned(),
            submitted_at_ms: unix_millis(context.submitted_at),
            start_at_ms: unix_millis(start_at),
            scoring_icpc: context.scoring_icpc,
            aggregation_best: context.aggregation_best,
            score_milli,
            max_score_milli: context.max_score_milli,
        };
        if let Err(error) = projection.apply_judgement(&event).await {
            tracing::warn!(
                ?error,
                contest_id,
                team_id,
                problem = context.problem_id,
                "scoreboard projection apply failed; marking cell dirty"
            );
            projection.mark_dirty(contest_id, team_id, context.problem_id).await;
        }
    }
}

fn unix_millis(at: OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}

async fn apply_practice_progress(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &ResultContext,
    accepted: bool,
) -> Result<(), sqlx::Error> {
    let user_id = context
        .participant_user_id
        .ok_or_else(|| sqlx::Error::Protocol("practice submission has no participant".into()))?;
    let score = if accepted { 100 } else { 0 };
    sqlx::query(
        r#"
        INSERT INTO practice_problem_progress
            (user_id,problem_id,attempts,best_score,solved,last_submission_id,solved_at)
        VALUES($1,$2,1,$3,$4,$5,CASE WHEN $4 THEN now() ELSE NULL END)
        ON CONFLICT(user_id,problem_id) DO UPDATE
            SET attempts=practice_problem_progress.attempts+1,
                best_score=GREATEST(practice_problem_progress.best_score,EXCLUDED.best_score),
                solved=practice_problem_progress.solved OR EXCLUDED.solved,
                last_submission_id=EXCLUDED.last_submission_id,
                solved_at=coalesce(practice_problem_progress.solved_at,EXCLUDED.solved_at),
                updated_at=now()
        "#,
    )
    .bind(user_id)
    .bind(context.problem_id)
    .bind(score)
    .bind(accepted)
    .bind(context.submission_id)
    .execute(&mut **transaction)
    .await?;
    if let Some(enrollment_id) = context.training_enrollment_id {
        sqlx::query(
            r#"
            INSERT INTO training_progress
                (enrollment_id,problem_id,status,attempts,best_score,solved_at)
            VALUES($1,$2,$3,1,$4,CASE WHEN $3='SOLVED' THEN now() ELSE NULL END)
            ON CONFLICT(enrollment_id,problem_id) DO UPDATE
                SET status=CASE WHEN training_progress.status='SOLVED' OR EXCLUDED.status='SOLVED'
                        THEN 'SOLVED' ELSE 'IN_PROGRESS' END,
                    attempts=training_progress.attempts+1,
                    best_score=GREATEST(training_progress.best_score,EXCLUDED.best_score),
                    solved_at=coalesce(training_progress.solved_at,EXCLUDED.solved_at),
                    updated_at=now()
            "#,
        )
        .bind(enrollment_id)
        .bind(context.problem_id)
        .bind(if accepted { "SOLVED" } else { "IN_PROGRESS" })
        .bind(score)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            r#"
            WITH enrollment_state AS (
                SELECT NOT EXISTS(
                           SELECT 1 FROM training_set_items i
                           WHERE i.set_id = e.set_id AND i.required
                               AND NOT EXISTS(
                                   SELECT 1 FROM training_progress p
                                   WHERE p.enrollment_id = e.id
                                       AND p.problem_id = i.problem_id AND p.status = 'SOLVED'
                               )
                       ) AS done
                FROM training_enrollments e
                WHERE e.id = $1
            )
            UPDATE training_enrollments e
            SET status = CASE WHEN s.done THEN 'COMPLETED' ELSE 'ACTIVE' END,
                completed_at = CASE WHEN s.done THEN coalesce(e.completed_at, now()) ELSE NULL END,
                updated_at = now()
            FROM enrollment_state s
            WHERE e.id = $1
            "#,
        )
        .bind(enrollment_id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use project_balloon_contracts::{
        JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeRunResult, JudgeVerdict,
    };
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use crate::features::judge_dispatch::result_processor::{
        ApplyResultError, ApplyResultOutcome, JudgeResultProcessor,
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn final_result_is_atomic_idempotent_and_cannot_be_overwritten(pool: PgPool) {
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Result Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert team");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at)
            VALUES (
                'Result Contest', 'RUNNING', 'PRIVATE',
                date_trunc('second', now()) - interval '2 hours',
                date_trunc('second', now()) + interval '1 hour',
                date_trunc('second', now()) + interval '2 hours'
            )
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('result-problem', 'Result Problem') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'STAR')")
            .bind(contest_id).bind(team_id).execute(&pool).await.expect("roster result team");
        sqlx::query("INSERT INTO contest_problems (contest_id, problem_id, alias, display_order, color) VALUES ($1, $2, 'A', 1, '#ff0000')")
            .bind(contest_id).bind(problem_id).execute(&pool).await.expect("assign result problem");
        for (offset_minutes, verdict) in [(10, "WRONG_ANSWER"), (20, "COMPILE_ERROR")] {
            let rejected_submission_id = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO submissions
                    (contest_id, problem_id, team_id, language, source_object_key,
                     source_size_bytes, source_sha256, status, verdict, submitted_at, judged_at)
                VALUES (
                    $1, $2, $3, 'cpp', $4, 10, $5, 'COMPLETED', $6,
                    (SELECT start_at + make_interval(mins => $7) FROM contests WHERE id = $1),
                    now()
                )
                RETURNING id
                "#,
            )
            .bind(contest_id)
            .bind(problem_id)
            .bind(team_id)
            .bind(format!("sources/rejected-{offset_minutes}.cpp"))
            .bind("b".repeat(64))
            .bind(verdict)
            .bind(offset_minutes)
            .fetch_one(&pool)
            .await
            .expect("insert earlier rejected submission");
            sqlx::query(
                r#"
                INSERT INTO judgements (id, submission_id, verdict, completed_at)
                VALUES ($1, $2, $3, now())
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(rejected_submission_id)
            .bind(verdict)
            .execute(&pool)
            .await
            .expect("insert earlier rejected judgement");
        }
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, submitted_at)
            VALUES (
                $1, $2, $3, 'cpp', 'sources/result.cpp', 10, $4, 'JUDGING',
                (SELECT start_at + interval '90 minutes' FROM contests WHERE id = $1)
            )
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind("a".repeat(64))
        .fetch_one(&pool)
        .await
        .expect("insert submission");
        let judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements (id, submission_id) VALUES ($1, $2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(&pool)
            .await
            .expect("insert judgement");

        let now = OffsetDateTime::now_utc();
        let result = JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: judgement_id,
            judgement_id,
            submission_id,
            worker_id: "worker-integration".to_owned(),
            verdict: JudgeVerdict::Accepted,
            total_time_ms: 35,
            peak_memory_kb: 2_048,
            compile_log: Some("compiled".to_owned()),
            started_at: now - Duration::SECOND,
            completed_at: now,
            runs: vec![
                JudgeRunResult {
                    test_index: 1,
                    verdict: JudgeVerdict::Accepted,
                    time_ms: 15,
                    memory_kb: 1_024,
                    exit_code: Some(0),
                    stderr_tail: None,
                },
                JudgeRunResult {
                    test_index: 2,
                    verdict: JudgeVerdict::Accepted,
                    time_ms: 20,
                    memory_kb: 2_048,
                    exit_code: Some(0),
                    stderr_tail: None,
                },
            ],
        };
        let processor = JudgeResultProcessor::new(pool.clone());
        let (first_delivery, duplicate_delivery) =
            tokio::join!(processor.apply(&result), processor.apply(&result));
        let outcomes = [
            first_delivery.expect("apply concurrent result"),
            duplicate_delivery.expect("concurrent duplicate is safe"),
        ];
        assert_eq!(
            outcomes.iter().filter(|outcome| **outcome == ApplyResultOutcome::Applied).count(),
            1
        );
        assert_eq!(
            outcomes.iter().filter(|outcome| **outcome == ApplyResultOutcome::Duplicate).count(),
            1
        );

        let persisted = sqlx::query_as::<_, (String, Option<Uuid>, i64, String)>(
            r#"
            SELECT j.verdict, j.result_message_id,
                   (SELECT count(*) FROM runs r WHERE r.judgement_id = j.id),
                   s.status
            FROM judgements j
            JOIN submissions s ON s.id = j.submission_id
            WHERE j.id = $1
            "#,
        )
        .bind(judgement_id)
        .fetch_one(&pool)
        .await
        .expect("load applied result");
        assert_eq!(persisted.0, "ACCEPTED");
        assert_eq!(persisted.1, Some(result.message_id));
        assert_eq!(persisted.2, 2);
        assert_eq!(persisted.3, "COMPLETED");
        let scoreboard = sqlx::query_as::<_, (i32, bool, i64, i32, i64)>(
            r#"
            SELECT cell.wrong_attempts, cell.solved, cell.penalty_minutes,
                   row.solved_count, row.penalty_minutes
            FROM contest_scoreboard_cells cell
            JOIN contest_scoreboard_rows row
              ON row.contest_id = cell.contest_id AND row.team_id = cell.team_id
            WHERE cell.contest_id = $1 AND cell.team_id = $2 AND cell.problem_id = $3
            "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("load authoritative scoreboard projection");
        assert_eq!(scoreboard, (1, true, 110, 1, 110));
        let balloon = sqlx::query_as::<_, (i64, String, bool, String)>(
            "SELECT count(*) OVER (), status, is_first_blood, color FROM balloon_tasks WHERE contest_id = $1 AND team_id = $2 AND problem_id = $3",
        )
        .bind(contest_id)
        .bind(team_id)
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("load atomic balloon task");
        assert_eq!(balloon, (1, "PENDING".to_owned(), true, "#ff0000".to_owned()));

        let mut conflicting = result;
        conflicting.message_id = Uuid::new_v4();
        let error = processor.apply(&conflicting).await.expect_err("overwrite must fail");
        assert!(matches!(error, ApplyResultError::Invalid(_)));
        assert!(error.is_permanent());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn superseded_judgement_cannot_apply_a_result(pool: PgPool) {
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Race Result Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert team");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at)
            VALUES (
                'Race Result Contest', 'RUNNING', 'PRIVATE',
                date_trunc('second', now()) - interval '2 hours',
                date_trunc('second', now()) + interval '1 hour',
                date_trunc('second', now()) + interval '2 hours'
            )
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('race-result', 'Race Result') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'STAR')")
            .bind(contest_id).bind(team_id).execute(&pool).await.expect("roster team");
        sqlx::query("INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)")
            .bind(contest_id).bind(problem_id).execute(&pool).await.expect("assign problem");
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, submitted_at)
            VALUES (
                $1, $2, $3, 'cpp', 'sources/race-result.cpp', 10, $4, 'JUDGING',
                (SELECT start_at + interval '30 minutes' FROM contests WHERE id = $1)
            )
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind("a".repeat(64))
        .fetch_one(&pool)
        .await
        .expect("insert submission");
        let active_judgement_id = Uuid::new_v4();
        let superseded_judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements (id, submission_id) VALUES ($1, $2)")
            .bind(active_judgement_id)
            .bind(submission_id)
            .execute(&pool)
            .await
            .expect("insert active judgement");
        // The unique index uq_judgements_one_active_per_submission only permits
        // one active_marker per submission, so the superseded row is inserted
        // in its final shape directly (same as rejudge.rs leaves it behind).
        sqlx::query(
            "INSERT INTO judgements (id, submission_id, superseded, active_marker) VALUES ($1, $2, true, NULL)",
        )
        .bind(superseded_judgement_id)
        .bind(submission_id)
        .execute(&pool)
        .await
        .expect("insert superseded judgement");

        let now = OffsetDateTime::now_utc();
        let build_result = |judgement_id: Uuid, verdict: JudgeVerdict| JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: judgement_id,
            judgement_id,
            submission_id,
            worker_id: "worker-race-result".to_owned(),
            verdict,
            total_time_ms: 10,
            peak_memory_kb: 1_024,
            compile_log: None,
            started_at: now - Duration::SECOND,
            completed_at: now,
            runs: vec![JudgeRunResult {
                test_index: 1,
                verdict,
                time_ms: 10,
                memory_kb: 1_024,
                exit_code: Some(1),
                stderr_tail: None,
            }],
        };
        let active_result = build_result(active_judgement_id, JudgeVerdict::WrongAnswer);
        let superseded_result = build_result(superseded_judgement_id, JudgeVerdict::Accepted);
        let processor = JudgeResultProcessor::new(pool.clone());
        let (active_outcome, superseded_outcome) =
            tokio::join!(processor.apply(&active_result), processor.apply(&superseded_result),);
        assert_eq!(active_outcome.expect("apply active result"), ApplyResultOutcome::Applied);
        assert_eq!(
            superseded_outcome.expect("apply superseded result is safe"),
            ApplyResultOutcome::Superseded,
            "the superseded judgement must refuse the result without erroring the consumer"
        );

        let persisted = sqlx::query_as::<_, (Option<String>, bool, Option<String>, Option<String>, String)>(
            r#"
            SELECT a.verdict, a.completed_at IS NOT NULL, b.verdict, b.completed_at::varchar, s.status
            FROM judgements a
            CROSS JOIN judgements b
            JOIN submissions s ON s.id = a.submission_id
            WHERE a.id = $1 AND b.id = $2
            "#,
        )
        .bind(active_judgement_id)
        .bind(superseded_judgement_id)
        .fetch_one(&pool)
        .await
        .expect("load post-race state");
        assert_eq!(persisted.0.as_deref(), Some("WRONG_ANSWER"));
        assert!(persisted.1, "the active judgement must be finalized");
        assert_eq!(persisted.2, None, "the superseded judgement must stay without a verdict");
        assert_eq!(persisted.3, None, "the superseded judgement must stay without a completion");
        assert_eq!(
            persisted.4, "COMPLETED",
            "the submission must reflect exactly the active judgement"
        );
    }
}
