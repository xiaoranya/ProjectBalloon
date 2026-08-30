use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{error, info};
use uuid::Uuid;

use crate::features::judge_dispatch::error::JudgeDispatchError;

#[async_trait]
pub trait JudgeTaskPublisher: Send + Sync {
    async fn publish(&self, message_id: Uuid, payload: &[u8]) -> Result<(), JudgeDispatchError>;
}

#[derive(Clone, Copy)]
pub struct SubmissionOutboxDispatcherConfig {
    pub poll_interval: Duration,
    pub lease: Duration,
    pub retry_base: Duration,
    pub batch_size: i64,
    pub max_attempts: i32,
}

#[derive(sqlx::FromRow)]
struct ClaimedRow {
    id: i64,
    submission_id: i64,
    judgement_id: Uuid,
    payload: String,
    attempts: i32,
}

pub struct SubmissionOutboxDispatcher {
    database: PgPool,
    publisher: Arc<dyn JudgeTaskPublisher>,
    config: SubmissionOutboxDispatcherConfig,
    instance_id: Uuid,
}

impl SubmissionOutboxDispatcher {
    #[must_use]
    pub fn new(
        database: PgPool,
        publisher: Arc<dyn JudgeTaskPublisher>,
        config: SubmissionOutboxDispatcherConfig,
    ) -> Self {
        Self { database, publisher, config, instance_id: Uuid::new_v4() }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(dispatch_error) = self.dispatch_once().await {
                        error!(%dispatch_error, "submission outbox dispatch failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        info!("submission outbox dispatcher stopped");
                        return;
                    }
                }
            }
        }
    }

    pub async fn dispatch_once(&self) -> Result<usize, sqlx::Error> {
        let claimed = self.claim().await?;
        let count = claimed.len();
        for row in claimed {
            match self.publisher.publish(row.judgement_id, row.payload.as_bytes()).await {
                Ok(()) => {
                    if let Err(error) = self.mark_sent(row.id).await {
                        error!(
                            outbox_id = row.id,
                            submission_id = row.submission_id,
                            judgement_id = %row.judgement_id,
                            %error,
                            "failed to mark judge task sent; continuing dispatch batch"
                        );
                    }
                }
                Err(publish_error) => {
                    if let Err(error) =
                        self.mark_failed(row.id, row.attempts, &publish_error.to_string()).await
                    {
                        error!(
                            outbox_id = row.id,
                            submission_id = row.submission_id,
                            judgement_id = %row.judgement_id,
                            %error,
                            "failed to mark judge task failed; continuing dispatch batch"
                        );
                    }
                }
            }
        }
        Ok(count)
    }

    async fn claim(&self) -> Result<Vec<ClaimedRow>, sqlx::Error> {
        let lease_seconds = i32::try_from(self.config.lease.as_secs()).unwrap_or(i32::MAX);
        sqlx::query_as::<_, ClaimedRow>(
            r#"
            WITH candidates AS (
                SELECT id
                FROM submission_outbox
                WHERE (
                    (status IN ('PENDING', 'FAILED')
                        AND attempts < $1
                        AND available_at <= now())
                    OR (status = 'PUBLISHING'
                        AND attempts <= $1
                        AND lease_until < now())
                )
                ORDER BY available_at, created_at, id
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE submission_outbox outbox
            SET status = 'PUBLISHING',
                attempts = outbox.attempts + 1,
                lease_owner = $3,
                lease_until = now() + make_interval(secs => $4),
                version = outbox.version + 1
            FROM candidates
            WHERE outbox.id = candidates.id
            RETURNING outbox.id, outbox.submission_id, outbox.judgement_id, outbox.payload, outbox.attempts
            "#,
        )
        .bind(self.config.max_attempts)
        .bind(self.config.batch_size)
        .bind(self.instance_id)
        .bind(lease_seconds)
        .fetch_all(&self.database)
        .await
    }

    async fn mark_sent(&self, id: i64) -> Result<(), sqlx::Error> {
        let mut transaction = self.database.begin().await?;
        let sent = sqlx::query_as::<_, (i64, Uuid)>(
            r#"
            UPDATE submission_outbox
            SET status = 'SENT', sent_at = now(), last_error = NULL,
                lease_owner = NULL, lease_until = NULL, version = version + 1
            WHERE id = $1 AND status = 'PUBLISHING' AND lease_owner = $2
            RETURNING submission_id, judgement_id
            "#,
        )
        .bind(id)
        .bind(self.instance_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some((submission_id, judgement_id)) = sent {
            let context = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
                r#"
                UPDATE submissions
                SET status = 'JUDGING'
                WHERE id = $1 AND status = 'PENDING'
                RETURNING contest_id, team_id
                "#,
            )
            .bind(submission_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some((Some(contest_id), Some(team_id))) = context {
                sqlx::query(
                    r#"
                    INSERT INTO realtime_outbox
                        (event_id, contest_id, event_type, scope, team_id, payload_json)
                    VALUES ($1, $2, 'SUBMISSION_STATUS_CHANGED', 'TEAM', $3, $4)
                    "#,
                )
                .bind(Uuid::new_v4())
                .bind(contest_id)
                .bind(team_id)
                .bind(serde_json::json!({
                    "submissionId": submission_id,
                    "judgementId": judgement_id,
                    "status": "JUDGING"
                }))
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn mark_failed(&self, id: i64, attempts: i32, message: &str) -> Result<(), sqlx::Error> {
        let delay = retry_delay(self.config.retry_base, attempts);
        let delay_milliseconds = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
        let safe_message: String = message.chars().take(1_000).collect();
        let terminal = attempts >= self.config.max_attempts;
        let mut transaction = self.database.begin().await?;
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE submission_outbox
            SET status = 'FAILED', last_error = $3,
                available_at = now() + $4 * interval '1 millisecond',
                lease_owner = NULL, lease_until = NULL, version = version + 1
            WHERE id = $1 AND status = 'PUBLISHING' AND lease_owner = $2
            RETURNING submission_id
            "#,
        )
        .bind(id)
        .bind(self.instance_id)
        .bind(safe_message)
        .bind(delay_milliseconds)
        .fetch_optional(&mut *transaction)
        .await?;
        if terminal && let Some(submission_id) = submission_id {
            let context = sqlx::query_as::<_, (i64, i64)>(
                    "UPDATE submissions SET status='COMPLETED', verdict='SYSTEM_ERROR', judged_at=now() WHERE id=$1 AND status='PENDING' RETURNING contest_id, team_id",
                )
                .bind(submission_id)
                .fetch_optional(&mut *transaction)
                .await?;
            if let Some((contest_id, team_id)) = context {
                sqlx::query(
                        "INSERT INTO realtime_outbox(event_id,contest_id,event_type,scope,team_id,payload_json) VALUES($1,$2,'SUBMISSION_STATUS_CHANGED','TEAM',$3,$4)",
                    )
                    .bind(Uuid::new_v4())
                    .bind(contest_id)
                    .bind(team_id)
                    .bind(serde_json::json!({
                        "submissionId": submission_id,
                        "status": "SYSTEM_ERROR",
                        "verdict": "SYSTEM_ERROR"
                    }))
                    .execute(&mut *transaction)
                    .await?;
            }
        }
        transaction.commit().await
    }
}

fn retry_delay(base: Duration, attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(6);
    base.saturating_mul(2_u32.saturating_pow(exponent))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::features::judge_dispatch::dispatcher::JudgeDispatchError;
    use crate::features::judge_dispatch::dispatcher::{
        JudgeTaskPublisher, SubmissionOutboxDispatcher, SubmissionOutboxDispatcherConfig,
        retry_delay,
    };

    #[test]
    fn retry_delay_is_exponential_and_capped() {
        let base = Duration::from_millis(500);
        assert_eq!(retry_delay(base, 1), Duration::from_millis(500));
        assert_eq!(retry_delay(base, 4), Duration::from_secs(4));
        assert_eq!(retry_delay(base, 99), Duration::from_secs(32));
    }

    #[derive(Default)]
    struct FakePublisher {
        calls: Mutex<Vec<Uuid>>,
        fail_once: Mutex<HashSet<Uuid>>,
    }

    #[async_trait]
    impl JudgeTaskPublisher for FakePublisher {
        async fn publish(
            &self,
            message_id: Uuid,
            _payload: &[u8],
        ) -> Result<(), JudgeDispatchError> {
            self.calls.lock().expect("publisher calls lock").push(message_id);
            if self.fail_once.lock().expect("publisher failures lock").remove(&message_id) {
                Err(JudgeDispatchError::Rejected("Judge task"))
            } else {
                Ok(())
            }
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn concurrent_dispatchers_claim_each_outbox_row_exactly_once(pool: PgPool) {
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Race Dispatch', 'RUNNING', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Race Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert team");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('race', 'Race') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let mut judgement_ids = Vec::new();
        for sequence in 1..=4 {
            let submission_id = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO submissions
                    (contest_id, problem_id, team_id, language, source_object_key,
                     source_size_bytes, status)
                VALUES ($1, $2, $3, 'cpp', $4, 1, 'PENDING')
                RETURNING id
                "#,
            )
            .bind(contest_id)
            .bind(problem_id)
            .bind(team_id)
            .bind(format!("fixture/race-{sequence}.cpp"))
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
            sqlx::query(
                "INSERT INTO submission_outbox (judgement_id, submission_id, payload) VALUES ($1, $2, $3)",
            )
            .bind(judgement_id)
            .bind(submission_id)
            .bind(format!(r#"{{"judgementId":"{judgement_id}"}}"#))
            .execute(&pool)
            .await
            .expect("insert outbox row");
            judgement_ids.push(judgement_id);
        }
        let publisher = Arc::new(FakePublisher::default());
        let config = SubmissionOutboxDispatcherConfig {
            poll_interval: Duration::from_millis(50),
            lease: Duration::from_secs(30),
            retry_base: Duration::from_secs(60),
            batch_size: 10,
            max_attempts: 2,
        };
        let first_dispatcher =
            SubmissionOutboxDispatcher::new(pool.clone(), publisher.clone(), config);
        let second_dispatcher =
            SubmissionOutboxDispatcher::new(pool.clone(), publisher.clone(), config);
        let first = tokio::spawn(async move { first_dispatcher.dispatch_once().await });
        let second = tokio::spawn(async move { second_dispatcher.dispatch_once().await });
        let (first_count, second_count) = tokio::join!(first, second);
        let first_count = first_count.expect("first dispatcher task").expect("first dispatch");
        let second_count = second_count.expect("second dispatcher task").expect("second dispatch");
        assert_eq!(
            first_count + second_count,
            4,
            "FOR UPDATE SKIP LOCKED must hand every row to exactly one dispatcher"
        );

        let rows = sqlx::query_as::<_, (Uuid, String, i32, bool)>(
            "SELECT judgement_id, status, attempts, lease_owner IS NULL FROM submission_outbox ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("load outbox rows");
        assert_eq!(rows.len(), 4, "no outbox row may be lost");
        let mut claimed_judgements = publisher.calls.lock().expect("publisher calls lock").clone();
        claimed_judgements.sort();
        claimed_judgements.dedup();
        assert_eq!(claimed_judgements.len(), 4, "each judgement must be published exactly once");
        let mut expected = judgement_ids;
        expected.sort();
        assert_eq!(claimed_judgements, expected);
        for row in &rows {
            assert_eq!(row.1, "SENT", "every row must end SENT");
            assert_eq!(row.2, 1, "sum(attempts) must equal the row count");
            assert!(row.3, "the winning lease must be released");
        }
        let judging = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM submissions WHERE status = 'JUDGING'",
        )
        .fetch_one(&pool)
        .await
        .expect("count judging submissions");
        assert_eq!(judging, 4);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn confirmed_publish_failure_and_expired_lease_have_safe_transitions(pool: PgPool) {
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Dispatch', 'RUNNING', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Dispatch Team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert team");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('dispatch', 'Dispatch') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let mut judgement_ids = Vec::new();
        for sequence in 1..=2 {
            let submission_id = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO submissions
                    (contest_id, problem_id, team_id, language, source_object_key,
                     source_size_bytes, status)
                VALUES ($1, $2, $3, 'cpp', $4, 1, 'PENDING')
                RETURNING id
                "#,
            )
            .bind(contest_id)
            .bind(problem_id)
            .bind(team_id)
            .bind(format!("fixture/source-{sequence}.cpp"))
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
            sqlx::query(
                "INSERT INTO submission_outbox (judgement_id, submission_id, payload) VALUES ($1, $2, $3)",
            )
            .bind(judgement_id)
            .bind(submission_id)
            .bind(format!(r#"{{"judgementId":"{judgement_id}"}}"#))
            .execute(&pool)
            .await
            .expect("insert outbox row");
            judgement_ids.push(judgement_id);
        }
        let publisher = Arc::new(FakePublisher::default());
        publisher.fail_once.lock().expect("publisher failures lock").insert(judgement_ids[1]);
        let dispatcher = SubmissionOutboxDispatcher::new(
            pool.clone(),
            publisher.clone(),
            SubmissionOutboxDispatcherConfig {
                poll_interval: Duration::from_millis(50),
                lease: Duration::from_secs(30),
                retry_base: Duration::from_secs(60),
                batch_size: 10,
                max_attempts: 2,
            },
        );
        assert_eq!(dispatcher.dispatch_once().await.expect("first dispatch"), 2);
        let states = sqlx::query_as::<_, (Uuid, String, i32, bool)>(
            "SELECT judgement_id, status, attempts, lease_owner IS NULL FROM submission_outbox ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("load first dispatch states");
        assert_eq!((states[0].1.as_str(), states[0].2, states[0].3), ("SENT", 1, true));
        assert_eq!((states[1].1.as_str(), states[1].2, states[1].3), ("FAILED", 1, true));

        sqlx::query(
            r#"
            UPDATE submission_outbox
            SET status = 'PUBLISHING', attempts = 2, lease_owner = $2,
                lease_until = now() - interval '1 second'
            WHERE judgement_id = $1
            "#,
        )
        .bind(judgement_ids[1])
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("simulate crashed publisher lease");
        assert_eq!(dispatcher.dispatch_once().await.expect("lease recovery dispatch"), 1);
        let recovered = sqlx::query_as::<_, (String, i32, bool)>(
            "SELECT status, attempts, lease_owner IS NULL FROM submission_outbox WHERE judgement_id = $1",
        )
        .bind(judgement_ids[1])
        .fetch_one(&pool)
        .await
        .expect("load recovered row");
        assert_eq!((recovered.0.as_str(), recovered.1, recovered.2), ("SENT", 3, true));
        assert_eq!(
            publisher.calls.lock().expect("publisher calls lock").as_slice(),
            &[judgement_ids[0], judgement_ids[1], judgement_ids[1]]
        );
    }
}
