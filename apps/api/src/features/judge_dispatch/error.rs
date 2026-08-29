//! Shared transport-level errors for the judge dispatch AMQP components.
//! Per-message business failures keep their dedicated error types
//! ([`ApplyResultError`], [`DeadLetterError`], [`HeartbeatProcessError`]);
//! these variants cover the broker session lifecycle that every consumer
//! and publisher shares.

use thiserror::Error;

use crate::features::judge_dispatch::{
    dead_letter_consumer::DeadLetterError, heartbeat_processor::HeartbeatProcessError,
    result_processor::ApplyResultError,
};

#[derive(Debug, Error)]
pub enum JudgeDispatchError {
    #[error("RabbitMQ {0} timed out")]
    Timeout(&'static str),
    #[error(transparent)]
    Amqp(#[from] lapin::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error("RabbitMQ cancelled the {0} consumer")]
    ConsumerCancelled(&'static str),
    #[error("RabbitMQ channel is unavailable")]
    ChannelUnavailable,
    #[error("RabbitMQ rejected or returned the {0}")]
    Rejected(&'static str),
    #[error(transparent)]
    ResultProcessing(#[from] ApplyResultError),
    #[error(transparent)]
    HeartbeatProcessing(#[from] HeartbeatProcessError),
    #[error(transparent)]
    DeadLetterRecovery(#[from] DeadLetterError),
}
