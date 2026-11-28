use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use redis::{AsyncCommands, aio::ConnectionManager};
use sha2::{Digest, Sha256};
use tokio::{sync::OwnedMutexGuard, time::Instant, time::timeout};
use tracing::warn;

use crate::features::scoreboard::model::{ScoreboardResponse, ValidatedScoreboardQuery};

/// How long Redis is assumed to be down after a failed read. Without this every
/// request paid the full operation timeout before it could rebuild, which turned
/// a Redis outage into an API-wide latency spike.
const UNAVAILABLE_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct ScoreboardCache {
    connection: ConnectionManager,
    ttl: Duration,
    operation_timeout: Duration,
    shared: Arc<CacheShared>,
}

#[derive(Default)]
struct CacheShared {
    /// One lock per cache key: concurrent misses on the same board collapse into
    /// a single PostgreSQL rebuild instead of one per waiting request.
    flights: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    /// `Some(instant)` while Redis is considered unreachable.
    unavailable_until: Mutex<Option<Instant>>,
}

impl CacheShared {
    async fn flight(&self, key: String) -> OwnedMutexGuard<()> {
        let lock = {
            let mut flights = self.flights.lock().expect("scoreboard cache flights");
            // Prune entries no request is holding any more.
            flights.retain(|_, weak| weak.strong_count() > 0);
            if let Some(existing) = flights.get(&key).and_then(Weak::upgrade) {
                existing
            } else {
                let created = Arc::new(tokio::sync::Mutex::new(()));
                flights.insert(key, Arc::downgrade(&created));
                created
            }
        };
        lock.lock_owned().await
    }
}

impl ScoreboardCache {
    pub async fn connect(
        redis_url: &str,
        ttl: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self { connection, ttl, operation_timeout, shared: Arc::new(CacheShared::default()) })
    }

    /// Serializes rebuilds of one cache key.
    ///
    /// The returned guard is held across the rebuild; whoever arrives second
    /// blocks here and then usually finds the freshly cached board.
    pub async fn flight(
        &self,
        contest_id: i64,
        revision: &str,
        variant: &str,
        phase: &str,
        query: &ValidatedScoreboardQuery,
    ) -> OwnedMutexGuard<()> {
        self.shared.flight(cache_key(contest_id, revision, variant, phase, query)).await
    }

    /// True while a previous failure says Redis is still unreachable.
    fn is_unavailable(&self) -> bool {
        let mut until = self.shared.unavailable_until.lock().expect("scoreboard cache state");
        match *until {
            Some(deadline) if Instant::now() < deadline => true,
            Some(_) => {
                *until = None;
                false
            }
            None => false,
        }
    }

    fn mark_unavailable(&self) {
        let mut until = self.shared.unavailable_until.lock().expect("scoreboard cache state");
        *until = Some(Instant::now() + UNAVAILABLE_BACKOFF);
    }

    pub async fn get(
        &self,
        contest_id: i64,
        revision: &str,
        variant: &str,
        phase: &str,
        query: &ValidatedScoreboardQuery,
    ) -> Option<ScoreboardResponse> {
        if self.is_unavailable() {
            return None;
        }
        let key = cache_key(contest_id, revision, variant, phase, query);
        let mut connection = self.connection.clone();
        let payload: Option<String> =
            match timeout(self.operation_timeout, connection.get(&key)).await {
                Ok(Ok(payload)) => payload,
                Ok(Err(error)) => {
                    warn!(%error, %key, "scoreboard Redis read failed; rebuilding from PostgreSQL");
                    self.mark_unavailable();
                    return None;
                }
                Err(_) => {
                    warn!(%key, "scoreboard Redis read timed out; rebuilding from PostgreSQL");
                    self.mark_unavailable();
                    return None;
                }
            };
        let payload = payload?;
        match serde_json::from_str(&payload) {
            Ok(board) => Some(board),
            Err(error) => {
                warn!(%error, %key, "discarding invalid scoreboard Redis payload");
                let _: Result<_, _> =
                    timeout(self.operation_timeout, connection.del::<_, usize>(&key)).await;
                None
            }
        }
    }

    /// `immutable` says the payload came from the freeze snapshot, which no
    /// later write can change; only that variant earns the extended TTL.
    pub async fn put(
        &self,
        revision: &str,
        phase: &str,
        query: &ValidatedScoreboardQuery,
        board: &ScoreboardResponse,
        immutable: bool,
    ) {
        let key = cache_key(board.contest_id, revision, &board.variant, phase, query);
        let payload = match serde_json::to_string(board) {
            Ok(payload) => payload,
            Err(error) => {
                warn!(%error, "failed to encode scoreboard Redis payload");
                return;
            }
        };
        let mut connection = self.connection.clone();
        // Immutable phases (the frozen board only counts submissions before
        // freeze_at) can be cached far longer than live boards — freeze is
        // exactly when public read pressure peaks.
        let ttl_seconds = if immutable && phase == "FROZEN" {
            self.ttl.as_secs().max(1) * 20
        } else {
            self.ttl.as_secs().max(1)
        };
        match timeout(
            self.operation_timeout,
            connection.set_ex::<_, _, ()>(&key, payload, ttl_seconds),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(%error, %key, "scoreboard Redis write failed; PostgreSQL remains authoritative");
                self.mark_unavailable();
            }
            Err(_) => {
                warn!(%key, "scoreboard Redis write timed out; PostgreSQL remains authoritative");
                self.mark_unavailable();
            }
        }
    }
}

fn cache_key(
    contest_id: i64,
    revision: &str,
    variant: &str,
    phase: &str,
    query: &ValidatedScoreboardQuery,
) -> String {
    let selector = format!(
        "{}\u{1f}{}",
        query.group_name.as_deref().unwrap_or(""),
        query.participation_type.as_deref().unwrap_or("")
    );
    let selector_hash = hex::encode(Sha256::digest(selector.as_bytes()));
    format!("xcpc:scoreboard:v1:{contest_id}:{revision}:{variant}:{phase}:{}", &selector_hash[..16])
}

#[cfg(test)]
mod tests {
    use std::{process::Command, time::Duration};

    use crate::features::scoreboard::cache::cache_key;
    use crate::features::scoreboard::model::ValidatedScoreboardQuery;

    #[test]
    fn key_separates_revision_variant_phase_and_filters() {
        let base = ValidatedScoreboardQuery { group_name: None, participation_type: None };
        let east = ValidatedScoreboardQuery {
            group_name: Some("East".to_owned()),
            participation_type: None,
        };
        assert_ne!(
            cache_key(1, "1:1", "PUBLIC", "LIVE", &base),
            cache_key(1, "1:2", "PUBLIC", "LIVE", &base)
        );
        assert_ne!(
            cache_key(1, "1:1", "PUBLIC", "LIVE", &base),
            cache_key(1, "1:1", "ADMIN", "LIVE", &base)
        );
        assert_ne!(
            cache_key(1, "1:1", "PUBLIC", "LIVE", &base),
            cache_key(1, "1:1", "PUBLIC", "FROZEN", &base)
        );
        assert_ne!(
            cache_key(1, "1:1", "PUBLIC", "LIVE", &base),
            cache_key(1, "1:1", "PUBLIC", "LIVE", &east)
        );
    }

    #[tokio::test]
    async fn concurrent_rebuilds_of_one_key_are_serialized() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let shared = Arc::new(super::CacheShared::default());
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let shared = shared.clone();
            let running = running.clone();
            let peak = peak.clone();
            handles.push(tokio::spawn(async move {
                let _guard = shared.flight("board".to_owned()).await;
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                running.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for handle in handles {
            handle.await.expect("flight task");
        }
        assert_eq!(peak.load(Ordering::SeqCst), 1, "only one rebuild may run per key");

        // A different key is independent.
        let first = shared.flight("a".to_owned()).await;
        let second = tokio::time::timeout(Duration::from_millis(50), shared.flight("b".to_owned()))
            .await
            .expect("a different key must not block");
        drop((first, second));
    }

    struct PausedContainer(String);
    impl Drop for PausedContainer {
        fn drop(&mut self) {
            let _status = Command::new("docker").args(["unpause", &self.0]).status();
        }
    }

    #[tokio::test]
    #[ignore = "pauses the Redis Docker container named by PROJECT_BALLOON_TEST_REDIS_CONTAINER"]
    async fn paused_redis_returns_a_bounded_cache_miss() {
        let redis_url = std::env::var("PROJECT_BALLOON_TEST_REDIS_URL")
            .expect("PROJECT_BALLOON_TEST_REDIS_URL is required");
        let container = std::env::var("PROJECT_BALLOON_TEST_REDIS_CONTAINER")
            .expect("PROJECT_BALLOON_TEST_REDIS_CONTAINER is required");
        let operation_timeout = Duration::from_millis(100);
        let cache =
            super::ScoreboardCache::connect(&redis_url, Duration::from_secs(30), operation_timeout)
                .await
                .expect("connect Redis before fault injection");
        let status = Command::new("docker")
            .args(["pause", &container])
            .status()
            .expect("pause Redis container");
        assert!(status.success());
        let _guard = PausedContainer(container);

        let started = tokio::time::Instant::now();
        let result = cache
            .get(
                1,
                "1:1",
                "PUBLIC",
                "LIVE",
                &ValidatedScoreboardQuery { group_name: None, participation_type: None },
            )
            .await;
        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
