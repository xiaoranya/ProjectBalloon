//! Shared Redis access layer.
//!
//! Every Redis-backed subsystem (login sessions, login rate limiting, the
//! realtime event outbox, the scoreboard projection) shares one
//! [`RedisHandle`]. The handle maintains a small pool of multiplexed
//! connections and distributes commands round-robin across them, so a hot
//! judgement path (projection `EVAL` + outbox `XADD`) never serializes behind
//! a large `HGETALL` sitting in one TCP socket's buffers. Each operation is
//! bounded by an operation timeout so a stalled Redis server can never wedge a
//! request handler, mirroring the fault-isolation contract of the scoreboard
//! cache.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use redis::{Cmd, FromRedisValue, Value, aio::ConnectionManager};
use tokio::time::timeout;
use tracing::warn;

use crate::error::AppError;

/// Default number of multiplexed connections behind one [`RedisHandle`].
/// Each connection is an independent TCP socket with driver-managed
/// reconnection, so throughput scales roughly linearly with this value until
/// the Redis server itself saturates.
pub const DEFAULT_REDIS_POOL_SIZE: usize = 8;

#[derive(Clone)]
pub struct RedisHandle {
    connections: Vec<ConnectionManager>,
    next: Arc<AtomicUsize>,
    operation_timeout: Duration,
}

impl RedisHandle {
    pub async fn connect(
        redis_url: &str,
        operation_timeout: Duration,
    ) -> Result<Self, redis::RedisError> {
        Self::connect_with_pool_size(redis_url, operation_timeout, DEFAULT_REDIS_POOL_SIZE).await
    }

    /// Opens `pool_size` independent multiplexed connections to Redis. Any
    /// failed connection aborts startup, matching the fail-fast contract of
    /// the single-connection startup path.
    pub async fn connect_with_pool_size(
        redis_url: &str,
        operation_timeout: Duration,
        pool_size: usize,
    ) -> Result<Self, redis::RedisError> {
        let pool_size = pool_size.max(1);
        let client = redis::Client::open(redis_url)?;
        let mut connections = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            connections.push(client.get_connection_manager().await?);
        }
        Ok(Self { connections, next: Arc::new(AtomicUsize::new(0)), operation_timeout })
    }

    /// Number of multiplexed connections behind this handle.
    #[must_use]
    pub fn pool_size(&self) -> usize {
        self.connections.len()
    }

    /// Picks the next connection round-robin. `ConnectionManager` is itself
    /// multiplexed and cheap to clone, so concurrent callers share each
    /// connection without blocking one another.
    fn connection(&self) -> ConnectionManager {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.connections.len();
        self.connections[index].clone()
    }

    /// Runs a command under the operation timeout and maps every failure to an
    /// [`AppError`]. Callers decide whether a Redis failure degrades the
    /// feature or fails the request.
    pub(crate) async fn query<T: FromRedisValue>(&self, cmd: &mut Cmd) -> Result<T, AppError> {
        let mut connection = self.connection();
        match timeout(self.operation_timeout, cmd.query_async(&mut connection)).await {
            Ok(result) => {
                result.map_err(|error| AppError::internal_message("redis command failed", error))
            }
            Err(_) => Err(AppError::internal_message(
                "redis command timed out",
                format!("exceeded {:?}", self.operation_timeout),
            )),
        }
    }

    /// Runs a pipeline under the operation timeout and maps every failure to
    /// an [`AppError`].
    pub(crate) async fn query_pipeline<T: FromRedisValue>(
        &self,
        pipeline: redis::Pipeline,
    ) -> Result<T, AppError> {
        let mut connection = self.connection();
        match timeout(self.operation_timeout, pipeline.query_async(&mut connection)).await {
            Ok(result) => {
                result.map_err(|error| AppError::internal_message("redis pipeline failed", error))
            }
            Err(_) => Err(AppError::internal_message(
                "redis pipeline timed out",
                format!("exceeded {:?}", self.operation_timeout),
            )),
        }
    }

    /// Runs a command under the operation timeout, returning the raw reply for
    /// commands (such as `XAUTOCLAIM`) whose shape the driver does not model.
    pub(crate) async fn query_value(&self, cmd: &mut Cmd) -> Result<Value, AppError> {
        self.query(cmd).await
    }

    /// Runs a command best-effort: failures and timeouts are logged and
    /// swallowed. Used where Redis is a hint channel (outbox fan-out) and a
    /// blip must never fail the surrounding business operation.
    #[allow(dead_code)]
    pub(crate) async fn fire_and_forget(&self, cmd: &mut Cmd, context: &'static str) {
        if let Err(error) = self.query::<()>(cmd).await {
            warn!(?error, context, "Redis operation failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redis::cmd;

    /// Pool construction against the integration Redis; skips when the
    /// integration endpoint is not configured (same contract as the other
    /// Redis-backed integration tests).
    #[tokio::test]
    async fn pool_opens_every_requested_connection() {
        let Ok(redis_url) = std::env::var("PROJECT_BALLOON_TEST_REDIS_URL") else {
            eprintln!("skipping: PROJECT_BALLOON_TEST_REDIS_URL is not set");
            return;
        };
        let handle = RedisHandle::connect_with_pool_size(&redis_url, Duration::from_millis(500), 4)
            .await
            .expect("connect integration Redis");
        assert_eq!(handle.pool_size(), 4);
        let reply: String =
            handle.query(&mut cmd("PING")).await.expect("PING must round-trip over the pool");
        assert_eq!(reply, "PONG");
    }
}
