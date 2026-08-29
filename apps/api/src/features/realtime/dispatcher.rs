use std::time::Duration;

use project_balloon_contracts::{RealtimeEvent, RealtimeScope};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::realtime::{fanout::RealtimePublisher, hub::RealtimeEnvelope};

#[derive(Debug, Clone, Copy)]
pub struct DispatcherConfig {
    pub poll_interval: Duration,
    pub lease: Duration,
    pub retry_base: Duration,
    pub batch_size: i64,
    pub max_attempts: i32,
}

#[derive(Clone)]
pub struct OutboxDispatcher {
    database: PgPool,
    publisher: RealtimePublisher,
    config: DispatcherConfig,
    instance_id: Uuid,
}

#[derive(Debug, FromRow)]
struct ClaimedEvent {
    id: i64,
    event_id: Uuid,
    contest_id: i64,
    event_type: String,
    schema_version: i16,
    scope: String,
    team_id: Option<i64>,
    payload_json: Value,
    attempts: i32,
    created_at: OffsetDateTime,
}

impl OutboxDispatcher {
    #[must_use]
    pub fn new(database: PgPool, publisher: RealtimePublisher, config: DispatcherConfig) -> Self {
        Self { database, publisher, config, instance_id: Uuid::new_v4() }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        info!("realtime outbox dispatcher started");

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match self.dispatch_batch().await {
                        Ok(count) if count > 0 => {
                            info!(count, "published realtime outbox batch");
                        }
                        Ok(_) => {}
                        Err(error) => {
                            error!(%error, "realtime outbox dispatch failed");
                        }
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        info!("realtime outbox dispatcher stopped");
    }

    pub async fn dispatch_batch(&self) -> Result<usize, sqlx::Error> {
        self.recover_expired_claims().await?;
        let rows = self.claim().await?;
        let mut published = 0;
        for row in rows {
            let Some(scope) = parse_scope(&row.scope) else {
                warn!(outbox_id = row.id, scope = %row.scope, "invalid realtime outbox scope");
                self.mark_failed(
                    row.id,
                    row.attempts,
                    "invalid realtime scope persisted in outbox",
                )
                .await?;
                continue;
            };
            if row.schema_version <= 0 {
                self.mark_failed(
                    row.id,
                    row.attempts,
                    "invalid realtime schema version persisted in outbox",
                )
                .await?;
                continue;
            }
            let envelope = RealtimeEnvelope {
                event: RealtimeEvent {
                    id: row.event_id,
                    version: row.schema_version.cast_unsigned(),
                    event_type: row.event_type,
                    scope,
                    contest_id: row.contest_id,
                    occurred_at: row.created_at,
                    payload: row.payload_json,
                },
                team_id: row.team_id,
            };
            if let Err(error) = self.publisher.publish(envelope).await {
                let message = format!("realtime fanout failed: {error}");
                warn!(outbox_id = row.id, %error, "realtime fanout failed; scheduling retry");
                self.mark_failed(row.id, row.attempts, &message).await?;
                continue;
            }
            self.mark_published(row.id).await?;
            published += 1;
        }
        Ok(published)
    }

    async fn recover_expired_claims(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE realtime_outbox
            SET status = 'FAILED',
                attempts = LEAST(attempts, $1),
                lease_owner = NULL,
                last_error = 'dispatcher lease expired before delivery confirmation'
            WHERE status = 'PUBLISHING' AND available_at <= now()
            "#,
        )
        .bind(recovery_attempt_cap(self.config.max_attempts))
        .execute(&self.database)
        .await?;
        Ok(())
    }

    async fn claim(&self) -> Result<Vec<ClaimedEvent>, sqlx::Error> {
        sqlx::query_as(
            r#"
            WITH candidates AS (
                SELECT id
                FROM realtime_outbox
                WHERE status IN ('PENDING', 'FAILED')
                  AND available_at <= now()
                  AND attempts < $1
                ORDER BY available_at, id
                FOR UPDATE SKIP LOCKED
                LIMIT $2
            )
            UPDATE realtime_outbox AS outbox
            SET status = 'PUBLISHING',
                attempts = outbox.attempts + 1,
                available_at = now() + $3 * interval '1 millisecond',
                lease_owner = $4,
                last_error = NULL
            FROM candidates
            WHERE outbox.id = candidates.id
            RETURNING outbox.id, outbox.event_id, outbox.contest_id,
                      outbox.event_type, outbox.schema_version, outbox.scope,
                      outbox.team_id, outbox.payload_json, outbox.attempts,
                      outbox.created_at
            "#,
        )
        .bind(self.config.max_attempts)
        .bind(self.config.batch_size)
        .bind(duration_millis(self.config.lease))
        .bind(self.instance_id)
        .fetch_all(&self.database)
        .await
    }

    async fn mark_published(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE realtime_outbox
            SET status = 'PUBLISHED',
                published_at = now(),
                lease_owner = NULL,
                last_error = NULL
            WHERE id = $1 AND status = 'PUBLISHING' AND lease_owner = $2
            "#,
        )
        .bind(id)
        .bind(self.instance_id)
        .execute(&self.database)
        .await?;
        Ok(())
    }

    async fn mark_failed(&self, id: i64, attempts: i32, message: &str) -> Result<(), sqlx::Error> {
        let delay = retry_delay(self.config.retry_base, attempts);
        sqlx::query(
            r#"
            UPDATE realtime_outbox
            SET status = 'FAILED',
                available_at = now() + $2 * interval '1 millisecond',
                lease_owner = NULL,
                last_error = $3
            WHERE id = $1 AND status = 'PUBLISHING' AND lease_owner = $4
            "#,
        )
        .bind(id)
        .bind(duration_millis(delay))
        .bind(message)
        .bind(self.instance_id)
        .execute(&self.database)
        .await?;
        Ok(())
    }
}

fn parse_scope(value: &str) -> Option<RealtimeScope> {
    match value {
        "PUBLIC" => Some(RealtimeScope::Public),
        "STAFF" => Some(RealtimeScope::Staff),
        "TEAM" => Some(RealtimeScope::Team),
        _ => None,
    }
}

fn duration_millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn retry_delay(base: Duration, attempts: i32) -> Duration {
    let exponent = u32::try_from(attempts.saturating_sub(1)).unwrap_or(0).min(6);
    base.saturating_mul(2_u32.saturating_pow(exponent)).min(Duration::from_secs(60))
}

fn recovery_attempt_cap(max_attempts: i32) -> i32 {
    max_attempts.saturating_sub(1).max(0)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::features::realtime::RealtimeHub;

    use crate::features::realtime::dispatcher::{
        DispatcherConfig, OutboxDispatcher, RealtimePublisher,
    };
    use crate::features::realtime::dispatcher::{parse_scope, recovery_attempt_cap, retry_delay};

    #[test]
    fn retry_delay_is_exponential_and_capped() {
        let base = Duration::from_secs(1);
        assert_eq!(retry_delay(base, 1), Duration::from_secs(1));
        assert_eq!(retry_delay(base, 4), Duration::from_secs(8));
        assert_eq!(retry_delay(base, 20), Duration::from_secs(60));
    }

    #[test]
    fn scope_parser_is_closed() {
        assert!(parse_scope("PUBLIC").is_some());
        assert!(parse_scope("private").is_none());
    }

    #[test]
    fn expired_claim_preserves_one_final_delivery_attempt() {
        assert_eq!(recovery_attempt_cap(8), 7);
        assert_eq!(recovery_attempt_cap(1), 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn expired_claims_recover_at_the_attempt_limit_and_old_owners_cannot_finish(
        pool: PgPool,
    ) {
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Realtime lease', 'RUNNING', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let event_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO realtime_outbox
                (event_id, contest_id, event_type, scope, payload_json, status, attempts, available_at, lease_owner)
            VALUES ($1, $2, 'TEST', 'PUBLIC', '{}'::jsonb, 'PUBLISHING', 2,
                    now() - interval '1 second', $3)
            "#,
        )
        .bind(event_id)
        .bind(contest_id)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("insert expired realtime claim");
        let dispatcher = OutboxDispatcher::new(
            pool.clone(),
            RealtimePublisher::local(RealtimeHub::new(8, false)),
            DispatcherConfig {
                poll_interval: Duration::from_millis(50),
                lease: Duration::from_secs(30),
                retry_base: Duration::from_secs(1),
                batch_size: 10,
                max_attempts: 2,
            },
        );
        assert_eq!(dispatcher.dispatch_batch().await.expect("recover claim"), 1);
        let state = sqlx::query_as::<_, (String, i32, bool)>(
            "SELECT status, attempts, lease_owner IS NULL FROM realtime_outbox WHERE event_id=$1",
        )
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .expect("load recovered realtime claim");
        assert_eq!(state, ("PUBLISHED".to_owned(), 2, true));

        let stale_event_id = Uuid::new_v4();
        let stale_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, payload_json) VALUES ($1, $2, 'STALE', 'PUBLIC', '{}'::jsonb) RETURNING id",
        )
        .bind(stale_event_id)
        .bind(contest_id)
        .fetch_one(&pool)
        .await
        .expect("insert stale-owner claim");
        assert_eq!(dispatcher.claim().await.expect("claim stale-owner row").len(), 1);
        let new_owner = Uuid::new_v4();
        sqlx::query("UPDATE realtime_outbox SET lease_owner=$2 WHERE id=$1")
            .bind(stale_id)
            .bind(new_owner)
            .execute(&pool)
            .await
            .expect("replace claim owner");
        dispatcher.mark_published(stale_id).await.expect("stale completion query");
        let state = sqlx::query_as::<_, (String, Uuid)>(
            "SELECT status, lease_owner FROM realtime_outbox WHERE id=$1",
        )
        .bind(stale_id)
        .fetch_one(&pool)
        .await
        .expect("load stale-owner claim");
        assert_eq!(state, ("PUBLISHING".to_owned(), new_owner));
    }
}
