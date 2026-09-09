//! Shared Redis access layer.
//!
//! Every Redis-backed subsystem (login sessions, login rate limiting, the
//! realtime event outbox) shares one multiplexed [`RedisHandle`]. Each
//! operation is bounded by an operation timeout so a stalled Redis server can
//! never wedge a request handler, mirroring the fault-isolation contract of
//! the scoreboard cache.

use std::time::Duration;

use redis::{Cmd, FromRedisValue, Value, aio::ConnectionManager};
use tokio::time::timeout;
use tracing::warn;

use crate::error::AppError;

#[derive(Clone)]
pub struct RedisHandle {
    connection: ConnectionManager,
    operation_timeout: Duration,
}

impl RedisHandle {
    pub async fn connect(
        redis_url: &str,
        operation_timeout: Duration,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self { connection, operation_timeout })
    }

    /// Runs a command under the operation timeout and maps every failure to an
    /// [`AppError`]. Callers decide whether a Redis failure degrades the
    /// feature or fails the request.
    pub(crate) async fn query<T: FromRedisValue>(&self, cmd: &mut Cmd) -> Result<T, AppError> {
        let mut connection = self.connection.clone();
        match timeout(self.operation_timeout, cmd.query_async(&mut connection)).await {
            Ok(result) => result.map_err(|error| {
                AppError::internal_message("redis command failed", error)
            }),
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
        let mut connection = self.connection.clone();
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
