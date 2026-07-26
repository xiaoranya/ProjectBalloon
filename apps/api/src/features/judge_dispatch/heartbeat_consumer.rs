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

use super::{heartbeat_processor::WorkerHeartbeatProcessor, topology};

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

    async fn consume_session(&self, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| "RabbitMQ heartbeat consumer connection timed out".to_owned())?
        .map_err(|error| error.to_string())?;
        let channel = connection.create_channel().await.map_err(|error| error.to_string())?;
        topology::declare(&channel).await.map_err(|error| error.to_string())?;
        channel
            .basic_qos(self.prefetch, BasicQosOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        let mut consumer = channel
            .basic_consume(
                JUDGE_HEARTBEATS_QUEUE.into(),
                format!("project-balloon-api-heartbeats-{}", Uuid::new_v4()).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let processor = WorkerHeartbeatProcessor::new(self.database.clone());
        loop {
            tokio::select! {
                delivery = consumer.next() => {
                    let Some(delivery) = delivery else { return Err("RabbitMQ cancelled the Worker heartbeat consumer".to_owned()); };
                    process_delivery(&processor, &delivery.map_err(|error| error.to_string())?).await?;
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
) -> Result<(), String> {
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
            .map_err(|error| error.to_string()),
        Err(error) => {
            warn!(%error, worker_id = %heartbeat.worker_id, "requeueing Worker heartbeat after database failure");
            delivery
                .nack(BasicNackOptions { multiple: false, requeue: true })
                .await
                .map_err(|ack_error| ack_error.to_string())?;
            Err(error.to_string())
        }
    }
}

async fn reject(delivery: &Delivery) -> Result<(), String> {
    delivery
        .reject(BasicRejectOptions { requeue: false })
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
