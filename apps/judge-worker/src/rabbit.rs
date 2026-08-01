use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use futures_util::StreamExt;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable, LongString, ShortString},
};
use project_balloon_contracts::{
    JUDGE_DEAD_EXCHANGE, JUDGE_DEAD_ROUTING_KEY, JUDGE_RESULT_ROUTING_KEY, JUDGE_RESULTS_EXCHANGE,
    JUDGE_TASKS_QUEUE, JudgeResult, JudgeTask,
};
use tokio::{sync::watch, task::JoinSet, time::timeout};
use tracing::{error, info, warn};

use crate::heartbeat::WorkerActivity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFailureKind {
    Retry,
    Dead,
}

#[derive(Debug)]
pub struct TaskFailure {
    pub kind: TaskFailureKind,
    pub reason: String,
}

impl TaskFailure {
    #[must_use]
    pub fn retry(reason: impl Into<String>) -> Self {
        Self { kind: TaskFailureKind::Retry, reason: reason.into() }
    }

    #[must_use]
    pub fn dead(reason: impl Into<String>) -> Self {
        Self { kind: TaskFailureKind::Dead, reason: reason.into() }
    }
}

#[async_trait]
pub trait JudgeTaskHandler: Send + Sync {
    async fn handle(&self, task: JudgeTask, retry_count: u32) -> Result<JudgeResult, TaskFailure>;
}

pub struct RabbitJudgeWorker {
    uri: String,
    task_queue: String,
    worker_id: String,
    prefetch: u16,
    request_timeout: Duration,
    reconnect_delay: Duration,
    handler: Arc<dyn JudgeTaskHandler>,
    activity: WorkerActivity,
}

pub struct RabbitJudgeWorkerConfig {
    pub uri: String,
    pub task_queue: String,
    pub worker_id: String,
    pub prefetch: u16,
    pub request_timeout: Duration,
    pub reconnect_delay: Duration,
}

impl RabbitJudgeWorker {
    #[must_use]
    pub fn new(
        config: RabbitJudgeWorkerConfig,
        handler: Arc<dyn JudgeTaskHandler>,
        activity: WorkerActivity,
    ) -> Self {
        Self {
            uri: config.uri,
            task_queue: config.task_queue,
            worker_id: config.worker_id,
            prefetch: config.prefetch,
            request_timeout: config.request_timeout,
            reconnect_delay: config.reconnect_delay,
            handler,
            activity,
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!(worker_id = %self.worker_id, prefetch = self.prefetch, "Judge task consumer started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            if let Err(reason) = self.consume_session(shutdown.clone()).await {
                error!(%reason, "Judge task consumer session failed");
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
        info!(worker_id = %self.worker_id, "Judge task consumer stopped");
    }

    async fn consume_session(&self, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| "RabbitMQ worker connection timed out".to_owned())?
        .map_err(|error| error.to_string())?;
        let channel = connection.create_channel().await.map_err(|error| error.to_string())?;
        verify_topology(&channel, &self.task_queue).await?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        channel
            .basic_qos(self.prefetch, BasicQosOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        let mut consumer = channel
            .basic_consume(
                self.task_queue.clone().into(),
                format!("project-balloon-worker-{}", self.worker_id).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut in_flight = JoinSet::new();
        loop {
            tokio::select! {
                delivery = consumer.next(), if in_flight.len() < usize::from(self.prefetch) => {
                    let Some(delivery) = delivery else {
                        return Err("RabbitMQ cancelled the Judge task consumer".to_owned());
                    };
                    let delivery = delivery.map_err(|error| error.to_string())?;
                    let channel = channel.clone();
                    let handler = self.handler.clone();
                    let activity = self.activity.clone();
                    let request_timeout = self.request_timeout;
                    in_flight.spawn(async move {
                        process_delivery(
                            &channel,
                            request_timeout,
                            handler.as_ref(),
                            &activity,
                            &delivery,
                        ).await
                    });
                }
                joined = in_flight.join_next(), if !in_flight.is_empty() => {
                    let Some(joined) = joined else { continue };
                    joined.map_err(|error| format!("Judge task execution panicked: {error}"))??;
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        info!(active_tasks = in_flight.len(), "stopped accepting Judge tasks; draining in-flight work");
                        return drain_in_flight(&mut in_flight).await;
                    }
                }
            }
        }
    }
}

async fn drain_in_flight(in_flight: &mut JoinSet<Result<(), String>>) -> Result<(), String> {
    let mut first_error = None;
    while let Some(joined) = in_flight.join_next().await {
        let result = joined.map_err(|error| format!("Judge task execution panicked: {error}"))?;
        if let Err(error) = result
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

async fn verify_topology(channel: &Channel, task_queue: &str) -> Result<(), String> {
    let passive_exchange = ExchangeDeclareOptions {
        passive: true,
        durable: true,
        ..ExchangeDeclareOptions::default()
    };
    for exchange in [JUDGE_RESULTS_EXCHANGE, JUDGE_DEAD_EXCHANGE] {
        channel
            .exchange_declare(
                exchange.into(),
                ExchangeKind::Direct,
                passive_exchange,
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    channel
        .queue_declare(
            task_queue.into(),
            QueueDeclareOptions { passive: true, durable: true, ..QueueDeclareOptions::default() },
            FieldTable::default(),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn process_delivery(
    channel: &Channel,
    request_timeout: Duration,
    handler: &dyn JudgeTaskHandler,
    activity: &WorkerActivity,
    delivery: &Delivery,
) -> Result<(), String> {
    let task = match serde_json::from_slice::<JudgeTask>(&delivery.data) {
        Ok(task) => task,
        Err(error) => {
            return dead_letter_and_ack(
                channel,
                request_timeout,
                delivery,
                &format!("malformed JudgeTask: {error}"),
            )
            .await;
        }
    };
    if let Err(error) = task.validate() {
        return dead_letter_and_ack(
            channel,
            request_timeout,
            delivery,
            &format!("invalid JudgeTask: {error}"),
        )
        .await;
    }
    if let Some(message_id) = delivery.properties.message_id().as_ref()
        && uuid::Uuid::parse_str(message_id.as_str()).ok() != Some(task.judgement_id)
    {
        return dead_letter_and_ack(
            channel,
            request_timeout,
            delivery,
            "AMQP message ID does not match judgementId",
        )
        .await;
    }

    let activity_guard = activity.begin_task();
    let handled = handler.handle(task.clone(), retry_count(delivery)).await;
    drop(activity_guard);
    match handled {
        Ok(result) => {
            if let Err(reason) = validate_handler_result(&task, &result) {
                return dead_letter_and_ack(channel, request_timeout, delivery, &reason).await;
            }
            let payload = serde_json::to_vec(&result).map_err(|error| error.to_string())?;
            if let Err(reason) = publish_persistent(
                channel,
                request_timeout,
                JUDGE_RESULTS_EXCHANGE,
                JUDGE_RESULT_ROUTING_KEY,
                result.message_id,
                &payload,
                FieldTable::default(),
            )
            .await
            {
                requeue(delivery).await?;
                return Err(reason);
            }
            delivery.ack(BasicAckOptions::default()).await.map_err(|error| error.to_string())?;
            info!(
                judgement_id = %result.judgement_id,
                verdict = result.verdict.as_str(),
                "Judge result confirmed; task acknowledged"
            );
            Ok(())
        }
        Err(failure) if failure.kind == TaskFailureKind::Dead => {
            dead_letter_and_ack(channel, request_timeout, delivery, &failure.reason).await
        }
        Err(failure) => {
            warn!(
                judgement_id = %task.judgement_id,
                reason = %failure.reason,
                "Judge task scheduled for broker retry"
            );
            delivery
                .nack(BasicNackOptions { multiple: false, requeue: false })
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        }
    }
}

/// RabbitMQ records each dead-letter cycle in `x-death`.  Count those cycles
/// before handing the task to the engine so permanent infrastructure failures
/// eventually become a terminal result instead of leaving a submission judging.
///
/// A single retry cycle dead-letters twice: once from the task queue (the
/// worker's `nack requeue:false`, reason `rejected`) and once from the retry
/// queue (TTL expiry back to the task queue, reason `expired`). RabbitMQ
/// records a separate `x-death` entry per reason per queue, so summing every
/// entry double-counts each cycle and halves the effective retry budget. Count
/// only the task-queue entries so `MAX_TASK_RETRIES` behaves as documented.
fn retry_count(delivery: &Delivery) -> u32 {
    let Some(headers) = delivery.properties.headers().as_ref() else { return 0 };
    let Some(AMQPValue::FieldArray(deaths)) = headers.inner().get("x-death") else {
        return 0;
    };
    deaths
        .as_slice()
        .iter()
        .filter_map(|entry| match entry {
            AMQPValue::FieldTable(death) => {
                let from_tasks_queue = match death.inner().get("queue") {
                    Some(AMQPValue::LongString(queue)) => {
                        queue.as_bytes() == JUDGE_TASKS_QUEUE.as_bytes()
                    }
                    Some(AMQPValue::ShortString(queue)) => queue.as_str() == JUDGE_TASKS_QUEUE,
                    _ => false,
                };
                from_tasks_queue.then(|| death.inner().get("count")).flatten()
            }
            _ => None,
        })
        .filter_map(|count| match count {
            AMQPValue::LongInt(value) => u32::try_from(*value).ok(),
            AMQPValue::LongLongInt(value) => u32::try_from(*value).ok(),
            _ => None,
        })
        .sum()
}

fn validate_handler_result(task: &JudgeTask, result: &JudgeResult) -> Result<(), String> {
    result.validate().map_err(|error| error.to_string())?;
    if result.judgement_id != task.judgement_id
        || result.submission_id != task.submission_id
        || result.message_id != task.judgement_id
    {
        return Err("Judge handler returned mismatched immutable identifiers".to_owned());
    }
    Ok(())
}

async fn dead_letter_and_ack(
    channel: &Channel,
    request_timeout: Duration,
    delivery: &Delivery,
    reason: &str,
) -> Result<(), String> {
    let safe_reason: String = reason.chars().take(1_000).collect();
    let mut headers = FieldTable::default();
    headers.insert(
        ShortString::from("failureReason"),
        AMQPValue::LongString(LongString::from(safe_reason.as_str())),
    );
    let message_id = delivery
        .properties
        .message_id()
        .as_ref()
        .and_then(|value| uuid::Uuid::parse_str(value.as_str()).ok())
        .unwrap_or_else(uuid::Uuid::new_v4);
    if let Err(error) = publish_persistent(
        channel,
        request_timeout,
        JUDGE_DEAD_EXCHANGE,
        JUDGE_DEAD_ROUTING_KEY,
        message_id,
        &delivery.data,
        headers,
    )
    .await
    {
        requeue(delivery).await?;
        return Err(error);
    }
    delivery.ack(BasicAckOptions::default()).await.map_err(|error| error.to_string())?;
    warn!(%safe_reason, "Judge task moved to dead-letter queue");
    Ok(())
}

async fn publish_persistent(
    channel: &Channel,
    request_timeout: Duration,
    exchange: &str,
    routing_key: &str,
    message_id: uuid::Uuid,
    payload: &[u8],
    mut headers: FieldTable,
) -> Result<(), String> {
    let message_id = message_id.to_string();
    headers.insert(
        ShortString::from("messageId"),
        AMQPValue::LongString(LongString::from(message_id.as_str())),
    );
    let properties = BasicProperties::default()
        .with_content_type("application/json".into())
        .with_delivery_mode(2)
        .with_message_id(message_id.into())
        .with_headers(headers);
    let confirm = timeout(
        request_timeout,
        channel.basic_publish(
            exchange.into(),
            routing_key.into(),
            BasicPublishOptions { mandatory: true, ..BasicPublishOptions::default() },
            payload,
            properties,
        ),
    )
    .await
    .map_err(|_| "RabbitMQ worker publish timed out".to_owned())?
    .map_err(|error| error.to_string())?;
    let confirmation = timeout(request_timeout, confirm)
        .await
        .map_err(|_| "RabbitMQ worker confirm timed out".to_owned())?
        .map_err(|error| error.to_string())?;
    if confirmation.is_ack() && confirmation.take_message().is_none() {
        Ok(())
    } else {
        Err("RabbitMQ rejected or returned the Worker publication".to_owned())
    }
}

async fn requeue(delivery: &Delivery) -> Result<(), String> {
    delivery
        .nack(BasicNackOptions { multiple: false, requeue: true })
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
