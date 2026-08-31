mod dead_letter_consumer;
mod dispatcher;
pub(crate) mod error;
mod heartbeat_consumer;
mod heartbeat_processor;
mod payload;
mod rabbit;
mod result_consumer;
mod result_processor;
mod stuck_reaper;
mod topology;

use std::{future::Future, time::Duration};

use tokio::time::timeout;

/// Bounds one AMQP channel-setup await so a broker that accepts TCP but stalls
/// on AMQP frames surfaces as a timeout (and a reconnect) instead of wedging
/// the publisher or a consumer session forever.
pub(crate) async fn within_request_timeout<T>(
    label: &'static str,
    budget: Duration,
    future: impl Future<Output = Result<T, lapin::Error>>,
) -> Result<T, JudgeDispatchError> {
    match timeout(budget, future).await {
        Ok(result) => result.map_err(JudgeDispatchError::from),
        Err(_) => Err(JudgeDispatchError::Timeout(label)),
    }
}

pub use dead_letter_consumer::RabbitDeadLetterConsumer;
pub use dispatcher::{SubmissionOutboxDispatcher, SubmissionOutboxDispatcherConfig};
pub use error::JudgeDispatchError;
pub use heartbeat_consumer::RabbitWorkerHeartbeatConsumer;
pub use rabbit::{RabbitJudgeProbe, RabbitJudgeTaskPublisher};
pub use result_consumer::RabbitJudgeResultConsumer;
pub use result_processor::{ApplyResultError, ApplyResultOutcome, JudgeResultProcessor};
pub use stuck_reaper::SubmissionStuckReaper;
