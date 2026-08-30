use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{info, warn};

/// How long a submission may sit in `JUDGING` after its judge task was
/// confirmed SENT before the reaper re-enqueues the task. Deliberately longer
/// than the 10-minute `SubmissionsStuckJudging` alert threshold so the alert
/// fires first and the reaper only closes the gap afterwards.
const STUCK_THRESHOLD: &str = "30 minutes";

/// Self-healing complement to the `SubmissionsStuckJudging` alert: re-enqueues
/// judge tasks whose submission stayed `JUDGING` far past the dispatch
/// lifetime even though the outbox row claims it was already sent.
///
/// Constraints (verified against the schema and the dispatcher):
/// - `submission_outbox` has `UNIQUE (judgement_id)`, so the reaper resets the
///   existing row instead of inserting a new one.
/// - `attempts` is never reset, so redispatched tasks remain capped by the
///   dispatcher's `max_attempts`; rows already at the cap are left for the
///   alert to page a human.
/// - Only SENT rows are touched: PENDING/PUBLISHING/FAILED rows are owned by
///   the dispatcher's lease and retry mechanisms.
/// - Re-applying a duplicate worker result is safe (the result processor
///   deduplicates by message id and rejects terminal submissions).
pub struct SubmissionStuckReaper {
    database: PgPool,
    poll_interval: Duration,
    max_attempts: i32,
}

impl SubmissionStuckReaper {
    #[must_use]
    pub const fn new(database: PgPool, poll_interval: Duration, max_attempts: i32) -> Self {
        Self { database, poll_interval, max_attempts }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.poll_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match self.requeue_stuck().await {
                        Ok(requeued) if requeued > 0 => {
                            info!(requeued, "re-enqueued judge tasks for stuck JUDGING submissions");
                        }
                        Ok(_) => {}
                        Err(error) => {
                            warn!(%error, "stuck-judging reaper sweep failed");
                        }
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        info!("stuck-judging reaper stopped");
                        return;
                    }
                }
            }
        }
    }

    /// Resets every stuck SENT outbox row to PENDING and returns how many.
    pub async fn requeue_stuck(&self) -> Result<u64, sqlx::Error> {
        let mut transaction = self.database.begin().await?;
        // Single transaction with FOR UPDATE SKIP LOCKED keeps concurrent API
        // instances from requeueing the same row twice.
        let stuck = sqlx::query_as::<_, (i64, i64, uuid::Uuid, i32)>(
            r#"
            SELECT o.id, s.id, o.judgement_id, o.attempts
            FROM submissions s
            JOIN submission_outbox o ON o.submission_id = s.id
            WHERE s.status = 'JUDGING'
              AND s.submitted_at < now() - $2::interval
              AND o.status = 'SENT'
              AND o.attempts < $1
            ORDER BY o.id
            FOR UPDATE OF o SKIP LOCKED
            LIMIT 100
            "#,
        )
        .bind(self.max_attempts)
        .bind(STUCK_THRESHOLD)
        .fetch_all(&mut *transaction)
        .await?;
        let mut requeued = 0;
        for (outbox_id, submission_id, judgement_id, attempts) in &stuck {
            sqlx::query(
                r#"
                UPDATE submission_outbox
                SET status = 'PENDING', available_at = now(), version = version + 1
                WHERE id = $1
                "#,
            )
            .bind(outbox_id)
            .execute(&mut *transaction)
            .await?;
            warn!(
                outbox_id,
                submission_id,
                %judgement_id,
                attempts,
                "re-enqueued judge task for a submission stuck in JUDGING"
            );
            requeued += 1;
        }
        transaction.commit().await?;
        Ok(requeued)
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::SubmissionStuckReaper;
    use crate::features::judge_dispatch::dispatcher::{
        JudgeTaskPublisher, SubmissionOutboxDispatcher, SubmissionOutboxDispatcherConfig,
    };

    struct CountingPublisher {
        calls: std::sync::Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl JudgeTaskPublisher for CountingPublisher {
        async fn publish(
            &self,
            message_id: Uuid,
            _payload: &[u8],
        ) -> Result<(), crate::features::judge_dispatch::error::JudgeDispatchError> {
            self.calls.lock().expect("publisher calls lock").push(message_id);
            Ok(())
        }
    }

    /// Seeds one stuck submission (JUDGING + SENT outbox row) plus its contest
    /// scaffolding. `submitted_at` and `attempts` are parameterized so each
    /// test can shape its own edge cases.
    async fn seed(
        pool: &PgPool,
        name: &str,
        submitted_ago: &str,
        attempts: i32,
        outbox_status: &str,
    ) -> (Uuid, i64) {
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ($1, 'RUNNING', 'PRIVATE') RETURNING id",
        )
        .bind(format!("Reaper {name}"))
        .fetch_one(pool)
        .await
        .expect("insert contest");
        let team_id =
            sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ($1) RETURNING id")
                .bind(format!("Reaper Team {name}"))
                .fetch_one(pool)
                .await
                .expect("insert team");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("reaper-{}", name.to_lowercase()))
        .fetch_one(pool)
        .await
        .expect("insert problem");
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, status, submitted_at)
            VALUES ($1, $2, $3, 'cpp', $4, 1, 'JUDGING', now() - $5::interval)
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind(format!("sources/reaper-{name}.cpp"))
        .bind(submitted_ago)
        .fetch_one(pool)
        .await
        .expect("insert submission");
        let judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements (id, submission_id) VALUES ($1, $2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(pool)
            .await
            .expect("insert judgement");
        sqlx::query(
            r#"
            INSERT INTO submission_outbox
                (judgement_id, submission_id, payload, status, attempts, sent_at)
            VALUES ($1, $2, $3, $4, $5, now())
            "#,
        )
        .bind(judgement_id)
        .bind(submission_id)
        .bind(format!(r#"{{"judgementId":"{judgement_id}"}}"#))
        .bind(outbox_status)
        .bind(attempts)
        .execute(pool)
        .await
        .expect("insert outbox row");
        (judgement_id, submission_id)
    }

    async fn outbox_state(pool: &PgPool, judgement_id: Uuid) -> (String, i32) {
        sqlx::query_as::<_, (String, i32)>(
            "SELECT status, attempts FROM submission_outbox WHERE judgement_id = $1",
        )
        .bind(judgement_id)
        .fetch_one(pool)
        .await
        .expect("load outbox row")
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn requeues_stuck_sent_row_and_dispatcher_claims_it_again(pool: PgPool) {
        let (judgement_id, _submission_id) = seed(&pool, "Stuck", "45 minutes", 1, "SENT").await;
        let reaper = SubmissionStuckReaper::new(pool.clone(), Duration::from_secs(60), 8);
        assert_eq!(reaper.requeue_stuck().await.expect("requeue"), 1);
        assert_eq!(outbox_state(&pool, judgement_id).await, ("PENDING".to_owned(), 1));

        // The existing dispatcher must be able to claim the requeued row.
        let publisher = Arc::new(CountingPublisher { calls: std::sync::Mutex::new(Vec::new()) });
        let dispatcher = SubmissionOutboxDispatcher::new(
            pool.clone(),
            publisher.clone(),
            SubmissionOutboxDispatcherConfig {
                poll_interval: Duration::from_millis(50),
                lease: Duration::from_secs(30),
                retry_base: Duration::from_secs(60),
                batch_size: 10,
                max_attempts: 8,
            },
        );
        assert_eq!(dispatcher.dispatch_once().await.expect("redispatch"), 1);
        assert_eq!(outbox_state(&pool, judgement_id).await, ("SENT".to_owned(), 2));
        assert_eq!(
            publisher.calls.lock().expect("publisher calls lock").as_slice(),
            &[judgement_id]
        );

        // The submission is still JUDGING and its task was sent again more
        // than 30 minutes after submission, so the next sweep legitimately
        // requeues it once more; each cycle bumps attempts until the
        // max_attempts cap hands the row back to the alert.
        assert_eq!(reaper.requeue_stuck().await.expect("second sweep"), 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn rows_at_max_attempts_are_left_for_the_alert(pool: PgPool) {
        let (judgement_id, _submission_id) = seed(&pool, "Exhausted", "2 hours", 8, "SENT").await;
        let reaper = SubmissionStuckReaper::new(pool.clone(), Duration::from_secs(60), 8);
        assert_eq!(reaper.requeue_stuck().await.expect("requeue"), 0);
        assert_eq!(outbox_state(&pool, judgement_id).await, ("SENT".to_owned(), 8));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn fresh_judging_submissions_are_untouched(pool: PgPool) {
        let (judgement_id, _submission_id) = seed(&pool, "Fresh", "2 minutes", 1, "SENT").await;
        let reaper = SubmissionStuckReaper::new(pool.clone(), Duration::from_secs(60), 8);
        assert_eq!(reaper.requeue_stuck().await.expect("requeue"), 0);
        assert_eq!(outbox_state(&pool, judgement_id).await, ("SENT".to_owned(), 1));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn rows_not_in_sent_state_are_untouched(pool: PgPool) {
        let (judgement_id, _submission_id) = seed(&pool, "Pending", "2 hours", 0, "PENDING").await;
        let reaper = SubmissionStuckReaper::new(pool.clone(), Duration::from_secs(60), 8);
        assert_eq!(reaper.requeue_stuck().await.expect("requeue"), 0);
        assert_eq!(outbox_state(&pool, judgement_id).await, ("PENDING".to_owned(), 0));
    }
}
