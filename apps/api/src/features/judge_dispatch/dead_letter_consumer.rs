use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties,
    message::Delivery,
    options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions},
    types::FieldTable,
};
use serde_json::json;
use sqlx::PgPool;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::judge_dispatch::{
    error::JudgeDispatchError, topology, within_request_timeout,
};

/// Consumes the `judge.dead` queue so dead-lettered tasks and permanently
/// rejected results never leave a submission stuck in `JUDGING`. Each dead
/// message is a `JudgeTask` (dead-lettered by a worker) or a `JudgeResult`
/// (rejected by the API result consumer); both carry the judgement and
/// submission they belong to. The affected submission is marked `SYSTEM_ERROR`
/// when it is genuinely stuck, then the message is acknowledged.
pub struct RabbitDeadLetterConsumer {
    database: PgPool,
    uri: String,
    request_timeout: Duration,
    reconnect_delay: Duration,
    prefetch: u16,
}

impl RabbitDeadLetterConsumer {
    #[must_use]
    pub const fn new(
        database: PgPool,
        uri: String,
        request_timeout: Duration,
        reconnect_delay: Duration,
        prefetch: u16,
    ) -> Self {
        Self { database, uri, request_timeout, reconnect_delay, prefetch }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!(prefetch = self.prefetch, "Judge dead-letter consumer started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            if let Err(reason) = self.consume_session(shutdown.clone()).await {
                error!(%reason, "Judge dead-letter consumer session failed");
            }
            tokio::select! {
                () = tokio::time::sleep(self.reconnect_delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        info!("Judge dead-letter consumer stopped");
    }

    async fn consume_session(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), JudgeDispatchError> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| JudgeDispatchError::Timeout("dead-letter consumer connection"))??;
        // Every channel-setup await is bounded so a stalling broker takes the
        // reconnect path instead of wedging the consumer task.
        let channel = within_request_timeout(
            "dead-letter consumer channel",
            self.request_timeout,
            connection.create_channel(),
        )
        .await?;
        within_request_timeout(
            "dead-letter consumer topology declaration",
            self.request_timeout,
            topology::declare(&channel),
        )
        .await?;
        within_request_timeout(
            "dead-letter consumer qos",
            self.request_timeout,
            channel.basic_qos(self.prefetch, BasicQosOptions::default()),
        )
        .await?;
        let consumer_tag = format!("project-balloon-api-dead-{}", Uuid::new_v4());
        let mut consumer = within_request_timeout(
            "dead-letter consumer subscription",
            self.request_timeout,
            channel.basic_consume(
                topology::DEAD_QUEUE.into(),
                consumer_tag.into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            ),
        )
        .await?;
        loop {
            tokio::select! {
                delivery = consumer.next() => {
                    let Some(delivery) = delivery else {
                        return Err(JudgeDispatchError::ConsumerCancelled("Judge dead-letter"));
                    };
                    let delivery = delivery?;
                    process_delivery(&self.database, &delivery).await?;
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum DeadLetterError {
    #[error("dead-letter cannot be recovered: {0}")]
    Permanent(String),
    #[error("database error while recovering dead-letter: {0}")]
    Database(#[from] sqlx::Error),
}

impl DeadLetterError {
    const fn is_permanent(&self) -> bool {
        matches!(self, Self::Permanent(_))
    }
}

#[derive(sqlx::FromRow)]
struct DeadLetterContext {
    submission_id: i64,
    completed: bool,
    superseded: bool,
    submission_scope: String,
    contest_id: Option<i64>,
    team_id: Option<i64>,
    status: String,
}

async fn process_delivery(
    database: &PgPool,
    delivery: &Delivery,
) -> Result<(), JudgeDispatchError> {
    let Some((judgement_id, submission_id)) = parse_dead_letter(&delivery.data) else {
        warn!("judge.dead message is neither a JudgeTask nor a JudgeResult; acknowledging");
        delivery.ack(BasicAckOptions::default()).await?;
        return Ok(());
    };
    match recover_stuck_submission(database, judgement_id, submission_id).await {
        Ok(()) => {
            delivery.ack(BasicAckOptions::default()).await?;
            info!(
                %judgement_id,
                submission_id,
                "dead-letter submission marked SYSTEM_ERROR and acknowledged"
            );
            Ok(())
        }
        Err(error) if error.is_permanent() => {
            warn!(
                %judgement_id,
                submission_id,
                %error,
                "dead-letter cannot be recovered; acknowledging"
            );
            delivery.ack(BasicAckOptions::default()).await?;
            Ok(())
        }
        Err(error) => {
            warn!(
                %judgement_id,
                submission_id,
                %error,
                "requeueing dead-letter after transient database failure"
            );
            delivery.nack(BasicNackOptions { multiple: false, requeue: true }).await?;
            Err(JudgeDispatchError::from(error))
        }
    }
}

fn parse_dead_letter(data: &[u8]) -> Option<(Uuid, i64)> {
    let value = serde_json::from_slice::<serde_json::Value>(data).ok()?;
    let judgement_id = value.get("judgementId")?.as_str()?.parse().ok()?;
    let submission_id = value.get("submissionId")?.as_i64()?;
    if submission_id <= 0 {
        return None;
    }
    Some((judgement_id, submission_id))
}

async fn recover_stuck_submission(
    database: &PgPool,
    judgement_id: Uuid,
    submission_id: i64,
) -> Result<(), DeadLetterError> {
    let mut transaction = database.begin().await?;
    let context = sqlx::query_as::<_, DeadLetterContext>(
        r#"
        SELECT j.submission_id,
               j.completed_at IS NOT NULL AS completed,
               j.superseded,
               s.submission_scope,
               s.contest_id,
               s.team_id,
               s.status
        FROM judgements j
        JOIN submissions s ON s.id = j.submission_id
        WHERE j.id = $1
        FOR UPDATE OF j, s
        "#,
    )
    .bind(judgement_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(context) = context else {
        return Err(DeadLetterError::Permanent(format!("unknown judgement {judgement_id}")));
    };
    if context.submission_id != submission_id {
        return Err(DeadLetterError::Permanent(format!(
            "judgement belongs to submission {}, not {}",
            context.submission_id, submission_id
        )));
    }
    // A result was already applied, the judgement was superseded by a rejudge,
    // or the submission already moved past judging — nothing is stuck.
    if context.completed
        || context.superseded
        || crate::features::submissions::SubmissionStatus::parse(&context.status)
            .is_some_and(crate::features::submissions::SubmissionStatus::is_terminal)
    {
        transaction.commit().await?;
        return Ok(());
    }
    let completed_at = OffsetDateTime::now_utc();
    sqlx::query(
        r#"
        UPDATE judgements
        SET verdict = 'SYSTEM_ERROR', completed_at = $2, version = version + 1
        WHERE id = $1
        "#,
    )
    .bind(judgement_id)
    .bind(completed_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE submissions SET status = 'COMPLETED', verdict = 'SYSTEM_ERROR', judged_at = $2 WHERE id = $1",
    )
        .bind(submission_id)
        .bind(completed_at)
        .execute(&mut *transaction)
        .await?;
    if context.submission_scope == "CONTEST" {
        let contest_id = context.contest_id.ok_or_else(|| {
            DeadLetterError::Permanent("contest submission has no contest".into())
        })?;
        let team_id = context
            .team_id
            .ok_or_else(|| DeadLetterError::Permanent("contest submission has no team".into()))?;
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
        .bind(json!({
            "submissionId": submission_id,
            "judgementId": judgement_id,
            "status": "SYSTEM_ERROR",
            "verdict": "SYSTEM_ERROR"
        }))
        .execute(&mut *transaction)
        .await?;
    }
    // Practice progress is not incremented here: a recovered SYSTEM_ERROR is a
    // best-effort terminal state, and the next genuine result for a rejudged
    // submission will reconcile the projection.
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES (NULL, 'JUDGE_DEAD_RECOVERED', 'submission', $1, '0.0.0.0', 'failed')
        "#,
    )
    .bind(submission_id.to_string())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::features::judge_dispatch::dead_letter_consumer::{
        DeadLetterError, parse_dead_letter, recover_stuck_submission,
    };

    #[test]
    fn dead_letter_parser_recovers_ids_from_invalid_payloads() {
        let judgement_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "judgementId": judgement_id,
            "submissionId": 42,
            "verdict": "NOT_A_VERDICT"
        });
        assert_eq!(parse_dead_letter(payload.to_string().as_bytes()), Some((judgement_id, 42)));
    }

    async fn seed_stuck_submission(pool: &PgPool) -> (Uuid, i64, i64) {
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('Dead Team') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("insert team");
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at)
            VALUES (
                'Dead Contest', 'RUNNING', 'PRIVATE',
                date_trunc('second', now()) - interval '2 hours',
                date_trunc('second', now()) + interval '1 hour',
                date_trunc('second', now()) + interval '2 hours'
            )
            RETURNING id
            "#,
        )
        .fetch_one(pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('dead-problem', 'Dead Problem') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("insert problem");
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id)
            .bind(team_id)
            .execute(pool)
            .await
            .expect("insert roster");
        sqlx::query("INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)")
            .bind(contest_id)
            .bind(problem_id)
            .execute(pool)
            .await
            .expect("assign problem");
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, submitted_at)
            VALUES ($1, $2, $3, 'cpp', 'sources/dead.cpp', 10, $4, 'JUDGING', now())
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind("a".repeat(64))
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
        (judgement_id, submission_id, contest_id)
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn stuck_submission_is_marked_system_error_and_ack_is_idempotent(pool: PgPool) {
        let (judgement_id, submission_id, contest_id) = seed_stuck_submission(&pool).await;

        recover_stuck_submission(&pool, judgement_id, submission_id)
            .await
            .expect("recover stuck submission");

        let (verdict, completed_at, submission_status) =
            sqlx::query_as::<_, (String, bool, String)>(
                r#"
            SELECT j.verdict, j.completed_at IS NOT NULL, s.status
            FROM judgements j
            JOIN submissions s ON s.id = j.submission_id
            WHERE j.id = $1
            "#,
            )
            .bind(judgement_id)
            .fetch_one(&pool)
            .await
            .expect("load recovered state");
        assert_eq!(verdict, "SYSTEM_ERROR");
        assert!(completed_at);
        assert_eq!(submission_status, "COMPLETED");

        let outbox = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*) FROM realtime_outbox
            WHERE contest_id = $1 AND event_type = 'SUBMISSION_STATUS_CHANGED'
              AND payload_json ->> 'judgementId' = $2::text
            "#,
        )
        .bind(contest_id)
        .bind(judgement_id)
        .fetch_one(&pool)
        .await
        .expect("count outbox events");
        assert_eq!(outbox, 1);

        // Recovery is idempotent: a second delivery must not mutate anything.
        recover_stuck_submission(&pool, judgement_id, submission_id)
            .await
            .expect("repeat recovery is a no-op");
        let unchanged = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM realtime_outbox WHERE contest_id = $1 AND event_type = 'SUBMISSION_STATUS_CHANGED'",
        )
        .bind(contest_id)
        .fetch_one(&pool)
        .await
        .expect("count outbox events again");
        assert_eq!(unchanged, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn practice_recovery_marks_system_error_without_realtime_event(pool: PgPool) {
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('practice-dead', 'test-only-hash', 'Practice Dead', 'SUPER_ADMIN')
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert user");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('practice-dead', 'Practice Dead') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, submitted_at,
                 submission_scope, participant_user_id)
            VALUES (NULL, $1, NULL, 'cpp', 'sources/practice-dead.cpp', 10, $2, 'JUDGING', now(),
                    'PRACTICE', $3)
            RETURNING id
            "#,
        )
        .bind(problem_id)
        .bind("a".repeat(64))
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert practice submission");
        let judgement_id = Uuid::new_v4();
        sqlx::query("INSERT INTO judgements (id, submission_id) VALUES ($1, $2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(&pool)
            .await
            .expect("insert judgement");

        recover_stuck_submission(&pool, judgement_id, submission_id)
            .await
            .expect("recover stuck practice submission");

        let submission_status =
            sqlx::query_scalar::<_, String>("SELECT status FROM submissions WHERE id = $1")
                .bind(submission_id)
                .fetch_one(&pool)
                .await
                .expect("load recovered practice submission");
        assert_eq!(submission_status, "COMPLETED");
        let outbox_events = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox")
            .fetch_one(&pool)
            .await
            .expect("count realtime outbox events");
        assert_eq!(outbox_events, 0, "PRACTICE recovery must not enqueue realtime events");

        recover_stuck_submission(&pool, judgement_id, submission_id)
            .await
            .expect("repeat recovery is a no-op");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn completed_judgement_is_left_untouched(pool: PgPool) {
        let (judgement_id, submission_id, _contest_id) = seed_stuck_submission(&pool).await;
        sqlx::query(
            "UPDATE judgements SET verdict = 'ACCEPTED', completed_at = now() WHERE id = $1",
        )
        .bind(judgement_id)
        .execute(&pool)
        .await
        .expect("finalize judgement");

        recover_stuck_submission(&pool, judgement_id, submission_id)
            .await
            .expect("completed judgement is a no-op");

        let verdict =
            sqlx::query_scalar::<_, String>("SELECT verdict FROM judgements WHERE id = $1")
                .bind(judgement_id)
                .fetch_one(&pool)
                .await
                .expect("load verdict");
        assert_eq!(verdict, "ACCEPTED");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn unknown_judgement_is_a_permanent_error(pool: PgPool) {
        let error = recover_stuck_submission(&pool, Uuid::new_v4(), 42)
            .await
            .expect_err("unknown judgement must fail");
        assert!(matches!(error, DeadLetterError::Permanent(_)));
    }
}
