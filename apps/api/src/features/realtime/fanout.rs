use std::{sync::Arc, time::Duration};

use futures_util::StreamExt;
use project_balloon_contracts::RealtimeEvent;
use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use super::hub::{RealtimeEnvelope, RealtimeHub};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedisMessage {
    origin_instance_id: Uuid,
    team_id: Option<i64>,
    event: RealtimeEvent,
}

#[derive(Clone)]
pub enum RealtimePublisher {
    Local { hub: RealtimeHub },
    Redis { hub: RealtimeHub, connection: ConnectionManager, channel: Arc<str>, instance_id: Uuid },
}

pub struct RedisSubscriber {
    client: redis::Client,
    channel: Arc<str>,
    instance_id: Uuid,
    hub: RealtimeHub,
    reconnect_delay: Duration,
}

#[derive(Debug, Error)]
pub enum FanoutError {
    #[error("serialize Redis realtime message: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("publish Redis realtime message: {0}")]
    Redis(#[from] redis::RedisError),
}

impl RealtimePublisher {
    #[must_use]
    pub const fn local(hub: RealtimeHub) -> Self {
        Self::Local { hub }
    }

    pub async fn connect_redis(
        redis_url: &str,
        channel: String,
        hub: RealtimeHub,
        reconnect_delay: Duration,
    ) -> Result<(Self, RedisSubscriber), redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_connection_manager().await?;
        let channel: Arc<str> = channel.into();
        let instance_id = Uuid::new_v4();
        let publisher =
            Self::Redis { hub: hub.clone(), connection, channel: channel.clone(), instance_id };
        let subscriber = RedisSubscriber { client, channel, instance_id, hub, reconnect_delay };
        Ok((publisher, subscriber))
    }

    pub async fn publish(&self, envelope: RealtimeEnvelope) -> Result<(), FanoutError> {
        match self {
            Self::Local { hub } => {
                hub.publish(envelope);
                Ok(())
            }
            Self::Redis { hub, connection, channel, instance_id } => {
                hub.publish(envelope.clone());
                let payload = serde_json::to_string(&RedisMessage {
                    origin_instance_id: *instance_id,
                    team_id: envelope.team_id,
                    event: envelope.event,
                })?;
                let mut connection = connection.clone();
                let _subscriber_count: usize =
                    connection.publish(channel.as_ref(), payload).await?;
                Ok(())
            }
        }
    }
}

impl RedisSubscriber {
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut consecutive_failures = 0_u32;
        loop {
            self.hub.set_redis_connected(false);
            let connection = tokio::select! {
                result = self.client.get_async_pubsub() => result,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                    continue;
                }
            };
            let mut pubsub = match connection {
                Ok(pubsub) => pubsub,
                Err(error) => {
                    warn!(%error, "failed to connect Redis realtime subscriber");
                    let delay = reconnect_delay(self.reconnect_delay, consecutive_failures);
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if wait_or_shutdown(delay, &mut shutdown).await {
                        break;
                    }
                    continue;
                }
            };
            if let Err(error) = pubsub.subscribe(self.channel.as_ref()).await {
                warn!(%error, "failed to subscribe Redis realtime channel");
                let delay = reconnect_delay(self.reconnect_delay, consecutive_failures);
                consecutive_failures = consecutive_failures.saturating_add(1);
                if wait_or_shutdown(delay, &mut shutdown).await {
                    break;
                }
                continue;
            }

            self.hub.set_redis_connected(true);
            consecutive_failures = 0;
            info!(channel = %self.channel, "Redis realtime subscriber connected");
            let mut messages = pubsub.into_on_message();
            loop {
                tokio::select! {
                    message = messages.next() => {
                        let Some(message) = message else {
                            warn!("Redis realtime subscription ended; reconnecting");
                            break;
                        };
                        match message.get_payload::<String>() {
                            Ok(payload) => self.receive(&payload),
                            Err(error) => warn!(%error, "ignoring invalid Redis realtime payload"),
                        }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            self.hub.set_redis_connected(false);
                            return;
                        }
                    }
                }
            }
            let delay = reconnect_delay(self.reconnect_delay, consecutive_failures);
            consecutive_failures = consecutive_failures.saturating_add(1);
            if wait_or_shutdown(delay, &mut shutdown).await {
                break;
            }
        }
        self.hub.set_redis_connected(false);
        info!("Redis realtime subscriber stopped");
    }

    fn receive(&self, payload: &str) {
        match serde_json::from_str::<RedisMessage>(payload) {
            Ok(message) if message.origin_instance_id == self.instance_id => {}
            Ok(message) => self
                .hub
                .publish(RealtimeEnvelope { event: message.event, team_id: message.team_id }),
            Err(error) => warn!(%error, "ignoring malformed Redis realtime message"),
        }
    }
}

fn reconnect_delay(base: Duration, consecutive_failures: u32) -> Duration {
    base.saturating_mul(2_u32.saturating_pow(consecutive_failures.min(5)))
        .min(Duration::from_secs(30))
}

async fn wait_or_shutdown(delay: Duration, shutdown: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = shutdown.changed() => changed.is_err() || *shutdown.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use project_balloon_contracts::{RealtimeEvent, RealtimeScope};
    use uuid::Uuid;

    use super::{RedisMessage, reconnect_delay};

    #[test]
    fn redis_wire_message_matches_the_previous_java_shape() {
        let message = RedisMessage {
            origin_instance_id: Uuid::nil(),
            team_id: Some(12),
            event: RealtimeEvent::connected(7, RealtimeScope::Team),
        };
        let value = serde_json::to_value(message).expect("wire message must serialize");

        assert_eq!(value["originInstanceId"], Uuid::nil().to_string());
        assert_eq!(value["teamId"], 12);
        assert_eq!(value["event"]["scope"], "TEAM");
    }

    #[test]
    fn subscriber_reconnect_backoff_is_bounded() {
        let base = Duration::from_secs(1);
        assert_eq!(reconnect_delay(base, 0), Duration::from_secs(1));
        assert_eq!(reconnect_delay(base, 3), Duration::from_secs(8));
        assert_eq!(reconnect_delay(base, 20), Duration::from_secs(30));
    }
}
