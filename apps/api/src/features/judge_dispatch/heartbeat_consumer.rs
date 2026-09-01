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
use project_balloon_contracts::JUDGE_HEARTBEATS_QUEUE;
use sqlx::PgPool;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::judge_dispatch::{
    error::JudgeDispatchError,
    heartbeat_processor::WorkerHeartbeatProcessor,
    payload::{HeartbeatPayload, message_id_mismatch, parse_heartbeat},
    topology, within_request_timeout,
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
        // Every channel-setup await is bounded so a stalling broker takes the
        // reconnect path instead of wedging the consumer task.
        let channel = within_request_timeout(
            "heartbeat consumer channel",
            self.request_timeout,
            connection.create_channel(),
        )
        .await?;
        within_request_timeout(
            "heartbeat consumer topology declaration",
            self.request_timeout,
            topology::declare(&channel),
        )
        .await?;
        within_request_timeout(
            "heartbeat consumer qos",
            self.request_timeout,
            channel.basic_qos(self.prefetch, BasicQosOptions::default()),
        )
        .await?;
        let mut consumer = within_request_timeout(
            "heartbeat consumer subscription",
            self.request_timeout,
            channel.basic_consume(
                JUDGE_HEARTBEATS_QUEUE.into(),
                format!("project-balloon-api-heartbeats-{}", Uuid::new_v4()).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            ),
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
    let heartbeat = match parse_heartbeat(&delivery.data) {
        HeartbeatPayload::Accepted(value) => value,
        HeartbeatPayload::Invalid(value) => {
            warn!(worker_id = %value.worker_id, "rejecting invalid Worker heartbeat");
            return reject(delivery).await;
        }
        HeartbeatPayload::Malformed(error) => {
            warn!(%error, "rejecting malformed Worker heartbeat");
            return reject(delivery).await;
        }
    };
    if message_id_mismatch(
        delivery.properties.message_id().as_ref().map(|value| value.as_str()),
        heartbeat.message_id,
    ) {
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
