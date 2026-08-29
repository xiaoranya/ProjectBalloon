use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use project_balloon_contracts::RealtimeEvent;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct RealtimeEnvelope {
    pub event: RealtimeEvent,
    pub team_id: Option<i64>,
}

#[derive(Clone)]
pub struct RealtimeHub {
    sender: broadcast::Sender<RealtimeEnvelope>,
    redis_enabled: bool,
    redis_connected: Arc<AtomicBool>,
}

impl RealtimeHub {
    #[must_use]
    pub fn new(capacity: usize, redis_enabled: bool) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender, redis_enabled, redis_connected: Arc::new(AtomicBool::new(false)) }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeEnvelope> {
        self.sender.subscribe()
    }

    pub fn publish(&self, envelope: RealtimeEnvelope) {
        // No active subscribers is a successful fanout: SSE messages are
        // invalidation hints and PostgreSQL remains the durable source.
        let _receiver_count = self.sender.send(envelope);
    }

    pub fn set_redis_connected(&self, connected: bool) {
        self.redis_connected.store(connected, Ordering::Release);
    }

    #[must_use]
    pub fn redis_status(&self) -> Option<bool> {
        self.redis_enabled.then(|| self.redis_connected.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use project_balloon_contracts::{RealtimeEvent, RealtimeScope};

    use crate::features::realtime::hub::{RealtimeEnvelope, RealtimeHub};

    #[tokio::test]
    async fn subscribers_receive_the_same_event_id() {
        let hub = RealtimeHub::new(4, false);
        let mut first = hub.subscribe();
        let mut second = hub.subscribe();
        let event = RealtimeEvent::connected(7, RealtimeScope::Staff);
        let event_id = event.id;

        hub.publish(RealtimeEnvelope { event, team_id: None });

        assert_eq!(first.recv().await.expect("first event").event.id, event_id);
        assert_eq!(second.recv().await.expect("second event").event.id, event_id);
    }

    #[test]
    fn redis_health_is_only_reported_when_enabled() {
        let local = RealtimeHub::new(4, false);
        assert_eq!(local.redis_status(), None);

        let redis = RealtimeHub::new(4, true);
        assert_eq!(redis.redis_status(), Some(false));
        redis.set_redis_connected(true);
        assert_eq!(redis.redis_status(), Some(true));
    }
}
