use std::time::Duration;

use project_balloon_contracts::RealtimeScope;
use tokio::{sync::watch, time::MissedTickBehavior};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::realtime::{fanout::RealtimePublisher, hub::RealtimeEnvelope, outbox::RealtimeOutbox};

#[derive(Debug, Clone, Copy)]
pub struct DispatcherConfig {
    pub poll_interval: Duration,
    pub lease: Duration,
    pub retry_base: Duration,
    pub batch_size: i64,
    pub max_attempts: i32,
}

/// Delivers realtime outbox events through a Redis consumer group.
///
/// Delivery semantics mirror the previous PostgreSQL outbox dispatcher:
/// - at-least-once: entries are acknowledged only after confirmed fan-out;
/// - a failed publish stays pending and is reclaimed after the lease window;
/// - entries exceeding the attempt budget are dead-lettered and counted.
/// `retry_base` is retained for configuration compatibility; Redis reclaims
/// retries on the lease interval instead of per-row backoff schedules.
#[derive(Clone)]
pub struct OutboxDispatcher {
    outbox: RealtimeOutbox,
    publisher: RealtimePublisher,
    config: DispatcherConfig,
    instance_id: String,
}

impl OutboxDispatcher {
    #[must_use]
    pub fn new(outbox: RealtimeOutbox, publisher: RealtimePublisher, config: DispatcherConfig) -> Self {
        Self {
            outbox,
            publisher,
            config,
            instance_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        if let Err(error) = self.outbox.ensure_consumer_group().await {
            error!(?error, "failed to create realtime outbox consumer group");
            return;
        }
        let mut ticker = tokio::time::interval(self.config.poll_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        info!("realtime outbox dispatcher started");
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(error) = self.dispatch_batch().await {
                        error!(?error, "realtime outbox dispatch failed");
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

    /// Delivers newly enqueued entries and reclaims expired leases. Returns
    /// the number of confirmed deliveries.
    pub async fn dispatch_batch(&self) -> Result<usize, crate::error::AppError> {
        let mut published = 0;
        let fresh = self
            .outbox
            .read_new(&self.instance_id, self.config.batch_size.max(1) as usize)
            .await?;
        for (stream_id, entry) in fresh {
            if self.deliver(&stream_id, entry).await? {
                published += 1;
            }
        }
        let reclaimed = self
            .outbox
            .reclaim_expired(&self.instance_id, self.config.lease, self.config.batch_size.max(1) as usize)
            .await?;
        for (stream_id, entry, delivery_count) in reclaimed {
            if delivery_count > i64::from(self.config.max_attempts) {
                warn!(
                    %stream_id,
                    event_id = %entry.event_id,
                    delivery_count,
                    "realtime outbox entry exhausted its attempt budget; dead-lettering"
                );
                self.outbox.record_dead_letter().await?;
                self.outbox.ack(&stream_id).await?;
                continue;
            }
            if self.deliver(&stream_id, entry).await? {
                published += 1;
            }
        }
        Ok(published)
    }

    /// Publishes one entry to the fan-out layer and acknowledges it. A failed
    /// publish leaves the entry pending for lease-based recovery.
    async fn deliver(
        &self,
        stream_id: &str,
        entry: crate::features::realtime::outbox::OutboxEntry,
    ) -> Result<bool, crate::error::AppError> {
        let Some(scope) = parse_scope(&entry.scope) else {
            warn!(%stream_id, scope = %entry.scope, "invalid realtime outbox scope");
            self.outbox.ack(stream_id).await?;
            return Ok(false);
        };
        let envelope = RealtimeEnvelope {
            event: project_balloon_contracts::RealtimeEvent {
                id: entry.event_id,
                version: entry.schema_version,
                event_type: entry.event_type,
                scope,
                contest_id: entry.contest_id,
                occurred_at: time::OffsetDateTime::from_unix_timestamp_nanos(
                    i128::from(entry.occurred_at_millis) * 1_000_000,
                )
                .map_err(|error| {
                    crate::error::AppError::internal("restore realtime event time", error)
                })?,
                payload: entry.payload,
            },
            team_id: entry.team_id,
        };
        if let Err(error) = self.publisher.publish(envelope).await {
            warn!(%stream_id, %error, "realtime fanout failed; entry stays pending for retry");
            return Ok(false);
        }
        self.outbox.ack(stream_id).await?;
        Ok(true)
    }
}

pub(crate) fn parse_scope(value: &str) -> Option<RealtimeScope> {
    match value {
        "PUBLIC" => Some(RealtimeScope::Public),
        "STAFF" => Some(RealtimeScope::Staff),
        "TEAM" => Some(RealtimeScope::Team),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::features::realtime::dispatcher::parse_scope;

    #[test]
    fn scope_parser_is_closed() {
        assert!(parse_scope("PUBLIC").is_some());
        assert!(parse_scope("private").is_none());
    }
}
