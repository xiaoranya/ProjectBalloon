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
use project_balloon_contracts::JudgeResult;
use sqlx::PgPool;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{error::JudgeDispatchError, result_processor::JudgeResultProcessor, topology};

pub struct RabbitJudgeResultConsumer {
    database: PgPool,
    uri: String,
    request_timeout: Duration,
    reconnect_delay: Duration,
    prefetch: u16,
}

impl RabbitJudgeResultConsumer {
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
        info!(prefetch = self.prefetch, "Judge result consumer started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            if let Err(reason) = self.consume_session(shutdown.clone()).await {
                error!(%reason, "Judge result consumer session failed");
            }
            tokio::select! {
                () = tokio::time::sleep(self.reconnect_delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        info!("Judge result consumer stopped");
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
        .map_err(|_| JudgeDispatchError::Timeout("result consumer connection"))??;
        let channel = connection.create_channel().await?;
        topology::declare(&channel).await?;
        channel.basic_qos(self.prefetch, BasicQosOptions::default()).await?;
        let consumer_tag = format!("project-balloon-api-results-{}", Uuid::new_v4());
        let mut consumer = channel
            .basic_consume(
                topology::RESULTS_QUEUE.into(),
                consumer_tag.into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        let processor = JudgeResultProcessor::new(self.database.clone());
        loop {
            tokio::select! {
                delivery = consumer.next() => {
                    let Some(delivery) = delivery else {
                        return Err(JudgeDispatchError::ConsumerCancelled("Judge result"));
                    };
                    let delivery = delivery?;
                    process_delivery(&processor, &delivery).await?;
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn process_delivery(
    processor: &JudgeResultProcessor,
    delivery: &Delivery,
) -> Result<(), JudgeDispatchError> {
    let result = match serde_json::from_slice::<JudgeResult>(&delivery.data) {
        Ok(result) => result,
        Err(error) => {
            warn!(%error, "rejecting malformed Judge result");
            reject_permanently(delivery).await?;
            return Ok(());
        }
    };
    if let Some(property_id) = delivery.properties.message_id().as_ref()
        && property_id.as_str() != result.message_id.to_string()
    {
        warn!(
            body_message_id = %result.message_id,
            property_message_id = %property_id,
            "rejecting Judge result with mismatched message IDs"
        );
        reject_permanently(delivery).await?;
        return Ok(());
    }
    match processor.apply(&result).await {
        Ok(outcome) => {
            delivery.ack(BasicAckOptions::default()).await?;
            info!(
                message_id = %result.message_id,
                judgement_id = %result.judgement_id,
                ?outcome,
                "Judge result transaction committed and acknowledged"
            );
            Ok(())
        }
        Err(error) if error.is_permanent() => {
            warn!(
                message_id = %result.message_id,
                judgement_id = %result.judgement_id,
                %error,
                "rejecting permanent Judge result failure"
            );
            reject_permanently(delivery).await
        }
        Err(error) => {
            warn!(
                message_id = %result.message_id,
                judgement_id = %result.judgement_id,
                %error,
                "requeueing Judge result after transient database failure"
            );
            delivery.nack(BasicNackOptions { multiple: false, requeue: true }).await?;
            Err(JudgeDispatchError::from(error))
        }
    }
}

async fn reject_permanently(delivery: &Delivery) -> Result<(), JudgeDispatchError> {
    delivery.reject(BasicRejectOptions { requeue: false }).await?;
    Ok(())
}
