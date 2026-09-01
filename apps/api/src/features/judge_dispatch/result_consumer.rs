use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties,
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
        BasicRejectOptions, ConfirmSelectOptions,
    },
    types::{AMQPValue, FieldTable, ShortString},
};
use project_balloon_contracts::JudgeResult;
use sqlx::PgPool;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::features::judge_dispatch::{
    error::JudgeDispatchError, payload::message_id_mismatch,
    result_processor::JudgeResultProcessor, topology, within_request_timeout,
};

/// A transiently failing result is parked on the retry queue (10 s TTL) and
/// dead-letters to `judge.dead` — which marks the submission `SYSTEM_ERROR` —
/// once this many processing attempts have been exhausted. Generous enough to
/// ride out a database failover, tight enough to terminate poison messages.
const MAX_RESULT_PROCESSING_RETRIES: i32 = 20;

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
        // Every channel-setup await is bounded: a broker that accepts TCP but
        // stalls on AMQP frames must take the reconnect path, not wedge the
        // consumer task.
        let channel = within_request_timeout(
            "result consumer channel",
            self.request_timeout,
            connection.create_channel(),
        )
        .await?;
        within_request_timeout(
            "result consumer topology declaration",
            self.request_timeout,
            topology::declare(&channel),
        )
        .await?;
        within_request_timeout(
            "result consumer qos",
            self.request_timeout,
            channel.basic_qos(self.prefetch, BasicQosOptions::default()),
        )
        .await?;
        // Publisher confirms on the republish path: a retry copy must be known
        // accepted by the broker before the original delivery is acknowledged,
        // otherwise a stalled broker could lose the result entirely. Consumer
        // acknowledgements are not publishes and never emit confirms.
        within_request_timeout(
            "result consumer confirm select",
            self.request_timeout,
            channel.confirm_select(ConfirmSelectOptions::default()),
        )
        .await?;
        let consumer_tag = format!("project-balloon-api-results-{}", Uuid::new_v4());
        let mut consumer = within_request_timeout(
            "result consumer subscription",
            self.request_timeout,
            channel.basic_consume(
                topology::RESULTS_QUEUE.into(),
                consumer_tag.into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            ),
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
                    process_delivery(
                        &channel,
                        &processor,
                        &delivery,
                        self.request_timeout,
                    ).await?;
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
    channel: &Channel,
    processor: &JudgeResultProcessor,
    delivery: &Delivery,
    request_timeout: Duration,
) -> Result<(), JudgeDispatchError> {
    let result = match serde_json::from_slice::<JudgeResult>(&delivery.data) {
        Ok(result) => result,
        Err(error) => {
            warn!(%error, "rejecting malformed Judge result");
            reject_permanently(delivery).await?;
            return Ok(());
        }
    };
    if let Some(property_id) = delivery.properties.message_id().as_ref().map(|value| value.as_str())
        && message_id_mismatch(Some(property_id), result.message_id)
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
                submission_id = result.submission_id,
                judgement_id = %result.judgement_id,
                ?outcome,
                "Judge result transaction committed and acknowledged"
            );
            Ok(())
        }
        Err(error) if error.is_permanent() => {
            warn!(
                message_id = %result.message_id,
                submission_id = result.submission_id,
                judgement_id = %result.judgement_id,
                %error,
                "rejecting permanent Judge result failure"
            );
            reject_permanently(delivery).await
        }
        Err(error) => {
            let retry_count = result_retry_count(delivery);
            if retry_count >= MAX_RESULT_PROCESSING_RETRIES {
                error!(
                    message_id = %result.message_id,
                    submission_id = result.submission_id,
                    judgement_id = %result.judgement_id,
                    retry_count,
                    %error,
                    "exhausted Judge result retries; dead-lettering for SYSTEM_ERROR recovery"
                );
                return reject_permanently(delivery).await;
            }
            defer_result_for_retry(channel, delivery, retry_count + 1, request_timeout).await?;
            delivery.ack(BasicAckOptions::default()).await?;
            warn!(
                message_id = %result.message_id,
                submission_id = result.submission_id,
                judgement_id = %result.judgement_id,
                retry_count = retry_count + 1,
                %error,
                "deferred Judge result to the retry queue after a transient failure"
            );
            Ok(())
        }
    }
}

/// Republishes the result onto the delayed-retry queue. The copy carries the
/// `x-retry-count` header so processing attempts stay bounded across the
/// results → retry → results cycles.
async fn defer_result_for_retry(
    channel: &Channel,
    delivery: &Delivery,
    next_retry_count: i32,
    request_timeout: Duration,
) -> Result<(), JudgeDispatchError> {
    let mut headers = FieldTable::default();
    headers.insert(ShortString::from("x-retry-count"), AMQPValue::LongInt(next_retry_count));
    let mut properties = BasicProperties::default()
        .with_content_type("application/json".into())
        .with_delivery_mode(2)
        .with_headers(headers);
    if let Some(message_id) = delivery.properties.message_id() {
        properties = properties.with_message_id(message_id.to_owned());
    }
    let confirm = timeout(
        request_timeout,
        channel.basic_publish(
            topology::RESULTS_RETRY_EXCHANGE.into(),
            topology::RESULTS_RETRY_ROUTING_KEY.into(),
            BasicPublishOptions::default(),
            delivery.data.as_slice(),
            properties,
        ),
    )
    .await
    .map_err(|_| JudgeDispatchError::Timeout("result retry republish"))??;
    timeout(request_timeout, confirm)
        .await
        .map_err(|_| JudgeDispatchError::Timeout("result retry publisher confirm"))??;
    Ok(())
}

/// Reads the processing-attempt counter from the retry-cycle headers.
fn result_retry_count(delivery: &Delivery) -> i32 {
    let Some(headers) = delivery.properties.headers().as_ref() else { return 0 };
    match headers.inner().get("x-retry-count") {
        Some(AMQPValue::LongInt(value)) => *value,
        Some(AMQPValue::ShortInt(value)) => i32::from(*value),
        Some(AMQPValue::LongLongInt(value)) => i32::try_from(*value).unwrap_or(i32::MAX),
        _ => 0,
    }
}

async fn reject_permanently(delivery: &Delivery) -> Result<(), JudgeDispatchError> {
    delivery.reject(BasicRejectOptions { requeue: false }).await?;
    Ok(())
}
