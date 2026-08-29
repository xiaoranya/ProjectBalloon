use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties,
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions, BasicRejectOptions,
    },
    types::FieldTable,
};
use project_balloon_contracts::{JUDGE_HEARTBEATS_QUEUE, WorkerHeartbeat};
use sqlx::PgPool;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::judge_dispatch::{
    error::JudgeDispatchError, heartbeat_processor::WorkerHeartbeatProcessor, topology,
};

pub struct RabbitWorkerHeartbeatConsumer {
    database: PgPool,
    uri: String,
    request_timeout: Duration,
    reconnect_delay: Duration,
    prefetch: u16,
}

impl RabbitWorkerHeartbeatConsumer {
    #[must_use]
    pub const fn new(
        database: PgPool,
        uri: String,
        request_timeout: Duration,
        reconnect_delay: Duration,
        prefetch: u16,
    ) -> Self {
        Self { database, uri, request_timeout, reconnect_delay, prefetch }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!("Worker heartbeat consumer started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            if let Err(reason) = self.consume_session(shutdown.clone()).await {
                error!(%reason, "Worker heartbeat consumer session failed");
            }
            tokio::select! {
                () = tokio::time::sleep(self.reconnect_delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { break; }
                }
            }
        }
        info!("Worker heartbeat consumer stopped");
    }

    async fn consume_session(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), JudgeDispatchError> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| JudgeDispatchError::Timeout("heartbeat consumer connection"))??;
        let channel = connection.create_channel().await?;
        topology::declare(&channel).await?;
        channel.basic_qos(self.prefetch, BasicQosOptions::default()).await?;
        let mut consumer = channel
            .basic_consume(
                JUDGE_HEARTBEATS_QUEUE.into(),
                format!("project-balloon-api-heartbeats-{}", Uuid::new_v4()).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        let processor = WorkerHeartbeatProcessor::new(self.database.clone());
        loop {
            tokio::select! {
                delivery = consumer.next() => {
                    let Some(delivery) = delivery
                    else { return Err(JudgeDispatchError::ConsumerCancelled("Worker heartbeat")); };
                    process_delivery(&processor, &delivery?).await?;
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return Ok(()); }
                }
            }
        }
    }
}

async fn process_delivery(
    processor: &WorkerHeartbeatProcessor,
    delivery: &Delivery,
) -> Result<(), JudgeDispatchError> {
    let heartbeat = match serde_json::from_slice::<WorkerHeartbeat>(&delivery.data) {
        Ok(value) if value.validate().is_ok() => value,
        Ok(value) => {
            warn!(worker_id = %value.worker_id, "rejecting invalid Worker heartbeat");
            return reject(delivery).await;
        }
        Err(error) => {
            warn!(%error, "rejecting malformed Worker heartbeat");
            return reject(delivery).await;
        }
    };
    if delivery
        .properties
        .message_id()
        .as_ref()
        .is_some_and(|id| id.as_str() != heartbeat.message_id.to_string())
    {
        warn!(worker_id = %heartbeat.worker_id, "rejecting Worker heartbeat with mismatched message IDs");
        return reject(delivery).await;
    }
    match processor.apply(&heartbeat).await {
        Ok(()) => delivery
            .ack(BasicAckOptions::default())
            .await
            .map(|_| ())
            .map_err(JudgeDispatchError::from),
        Err(error) => {
            warn!(%error, worker_id = %heartbeat.worker_id, "requeueing Worker heartbeat after database failure");
            delivery.nack(BasicNackOptions { multiple: false, requeue: true }).await?;
            Err(JudgeDispatchError::from(error))
        }
    }
}

async fn reject(delivery: &Delivery) -> Result<(), JudgeDispatchError> {
    delivery
        .reject(BasicRejectOptions { requeue: false })
        .await
        .map(|_| ())
        .map_err(JudgeDispatchError::from)
}
