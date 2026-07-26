use std::time::Duration;

use redis::{AsyncCommands, aio::ConnectionManager};
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use tracing::warn;

use super::model::{ScoreboardResponse, ValidatedScoreboardQuery};

#[derive(Clone)]
pub struct ScoreboardCache {
    connection: ConnectionManager,
    ttl: Duration,
    operation_timeout: Duration,
}

impl ScoreboardCache {
    pub async fn connect(
        redis_url: &str,
        ttl: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self { connection, ttl, operation_timeout })
    }

    pub async fn get(
        &self,
        contest_id: i64,
        revision: i64,
        variant: &str,
        phase: &str,
        query: &ValidatedScoreboardQuery,
    ) -> Option<ScoreboardResponse> {
        let key = cache_key(contest_id, revision, variant, phase, query);
        let mut connection = self.connection.clone();
        let payload: Option<String> =
            match timeout(self.operation_timeout, connection.get(&key)).await {
                Ok(Ok(payload)) => payload,
                Ok(Err(error)) => {
                    warn!(%error, %key, "scoreboard Redis read failed; rebuilding from PostgreSQL");
                    return None;
                }
                Err(_) => {
                    warn!(%key, "scoreboard Redis read timed out; rebuilding from PostgreSQL");
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

    pub async fn put(
        &self,
        revision: i64,
        phase: &str,
        query: &ValidatedScoreboardQuery,
        board: &ScoreboardResponse,
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
        let ttl_seconds = self.ttl.as_secs().max(1);
        match timeout(
            self.operation_timeout,
            connection.set_ex::<_, _, ()>(&key, payload, ttl_seconds),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(%error, %key, "scoreboard Redis write failed; PostgreSQL remains authoritative");
            }
            Err(_) => {
                warn!(%key, "scoreboard Redis write timed out; PostgreSQL remains authoritative");
            }
        }
    }
}

fn cache_key(
    contest_id: i64,
    revision: i64,
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

    use super::cache_key;
    use crate::features::scoreboard::model::ValidatedScoreboardQuery;

    #[test]
    fn key_separates_revision_variant_phase_and_filters() {
        let base = ValidatedScoreboardQuery { group_name: None, participation_type: None };
        let east = ValidatedScoreboardQuery {
            group_name: Some("East".to_owned()),
            participation_type: None,
        };
        assert_ne!(
            cache_key(1, 1, "PUBLIC", "LIVE", &base),
            cache_key(1, 2, "PUBLIC", "LIVE", &base)
        );
        assert_ne!(
            cache_key(1, 1, "PUBLIC", "LIVE", &base),
            cache_key(1, 1, "ADMIN", "LIVE", &base)
        );
        assert_ne!(
            cache_key(1, 1, "PUBLIC", "LIVE", &base),
            cache_key(1, 1, "PUBLIC", "FROZEN", &base)
        );
        assert_ne!(
            cache_key(1, 1, "PUBLIC", "LIVE", &base),
            cache_key(1, 1, "PUBLIC", "LIVE", &east)
        );
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
                1,
                "PUBLIC",
                "LIVE",
                &ValidatedScoreboardQuery { group_name: None, participation_type: None },
            )
            .await;
        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
