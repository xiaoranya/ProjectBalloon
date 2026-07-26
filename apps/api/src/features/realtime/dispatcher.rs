use std::time::Duration;

use project_balloon_contracts::{RealtimeEvent, RealtimeScope};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{fanout::RealtimePublisher, hub::RealtimeEnvelope};

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
    pub const fn new(
        database: PgPool,
        publisher: RealtimePublisher,
        config: DispatcherConfig,
    ) -> Self {
        Self { database, publisher, config }
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
                last_error = 'dispatcher lease expired before delivery confirmation'
            WHERE status = 'PUBLISHING' AND available_at <= now()
            "#,
        )
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
        .fetch_all(&self.database)
        .await
    }

    async fn mark_published(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE realtime_outbox
            SET status = 'PUBLISHED',
                published_at = now(),
                last_error = NULL
            WHERE id = $1 AND status = 'PUBLISHING'
            "#,
        )
        .bind(id)
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
                last_error = $3
            WHERE id = $1 AND status = 'PUBLISHING'
            "#,
        )
        .bind(id)
        .bind(duration_millis(delay))
        .bind(message)
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{parse_scope, retry_delay};

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
}
