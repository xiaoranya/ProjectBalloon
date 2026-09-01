use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures_util::StreamExt;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable, LongString, ShortString},
};
use project_balloon_contracts::{
    JUDGE_DEAD_EXCHANGE, JUDGE_DEAD_QUEUE, JUDGE_DEAD_ROUTING_KEY, JUDGE_HEARTBEAT_ROUTING_KEY,
    JUDGE_HEARTBEATS_EXCHANGE, JUDGE_HEARTBEATS_QUEUE, JUDGE_RESULT_ROUTING_KEY,
    JUDGE_RESULT_SCHEMA_VERSION, JUDGE_RESULTS_EXCHANGE, JUDGE_RESULTS_QUEUE,
    JUDGE_RESULTS_RETRY_EXCHANGE, JUDGE_RESULTS_RETRY_QUEUE, JUDGE_RETRY_EXCHANGE,
    JUDGE_RETRY_QUEUE, JUDGE_TASKS_EXCHANGE, JudgeMode, JudgeResult, JudgeTask, JudgeVerdict,
};
use time::OffsetDateTime;
use tokio::{sync::watch, task::JoinSet, time::timeout};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::health::HealthState;
use crate::heartbeat::WorkerActivity;
use crate::sandbox::{COMPILE_WALL_LIMIT, effective_time_limit, run_wall_limit};

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

/// The retry budget bounds how often a retryable failure may bounce back to
/// the queue before the task degrades into a SystemError verdict.
pub(crate) const MAX_TASK_RETRIES: u32 = 8;

pub(crate) fn retry_budget_exhausted(retry_count: u32) -> bool {
    retry_count >= MAX_TASK_RETRIES
}

/// Fixed margin on top of the sandbox's stage limits, covering artifact
/// acquisition, archive extraction, container lifecycle, and result
/// publication.
pub(crate) const TASK_DEADLINE_MARGIN: Duration = Duration::from_secs(300);

/// Worst-case wall clock of one Judge task, mirroring the sandbox's own stage
/// limits: the compile allowance plus, per test case, the same wall limit the
/// runner grants each run. `max_cases` bounds the case count because the task
/// contract does not carry it; output-only tasks run no container and only
/// compare bytes, so they get the margin alone.
pub(crate) fn task_deadline(task: &JudgeTask, max_cases: u32) -> Duration {
    if task.judge_mode == JudgeMode::OutputOnly {
        return TASK_DEADLINE_MARGIN;
    }
    let per_case =
        run_wall_limit(effective_time_limit(task.time_limit_ms, task.language_multiplier));
    COMPILE_WALL_LIMIT + TASK_DEADLINE_MARGIN + per_case.saturating_mul(max_cases)
}

/// What to do with a delivery whose handler missed its wall-clock deadline.
#[derive(Debug, PartialEq, Eq)]
enum DeadlineOutcome {
    Retry,
    SystemError,
}

/// Expired deadlines route through the existing retry budget: a transient
/// broker retry while the budget holds, a terminal SystemError verdict once it
/// is exhausted — never a crash loop, a lost slot, or a silent hang.
fn deadline_outcome(retry_count: u32) -> DeadlineOutcome {
    if retry_budget_exhausted(retry_count) {
        DeadlineOutcome::SystemError
    } else {
        DeadlineOutcome::Retry
    }
}

fn deadline_system_error(task: &JudgeTask, worker_id: &str, reason: &str) -> JudgeResult {
    let now = OffsetDateTime::now_utc();
    JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: task.judgement_id,
        judgement_id: task.judgement_id,
        submission_id: task.submission_id,
        worker_id: worker_id.to_owned(),
        verdict: JudgeVerdict::SystemError,
        total_time_ms: 0,
        peak_memory_kb: 0,
        compile_log: Some(reason.chars().take(1_000).collect()),
        started_at: now,
        completed_at: now,
        runs: Vec::new(),
    }
}

/// Registry of the judgements this worker is currently executing, each with
/// the wall-clock deadline it was granted. The orphan sweeper never touches
/// registered judgements, and the session drain derives its bound from the
/// longest remaining deadline.
#[derive(Clone, Default)]
pub struct InFlightTasks {
    entries: Arc<Mutex<HashMap<Uuid, Instant>>>,
}

impl InFlightTasks {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a judgement until the returned guard is dropped.
    pub(crate) fn guard(&self, judgement_id: Uuid, deadline: Instant) -> InFlightGuard {
        self.entries.lock().expect("in-flight task lock").insert(judgement_id, deadline);
        InFlightGuard { registry: self.clone(), judgement_id }
    }

    /// Snapshot of the registered judgement IDs.
    #[must_use]
    pub fn judgement_ids(&self) -> HashSet<Uuid> {
        self.entries.lock().expect("in-flight task lock").keys().copied().collect()
    }

    /// Longest remaining deadline among registered judgements, if any.
    #[must_use]
    pub fn max_remaining_deadline(&self) -> Option<Duration> {
        let entries = self.entries.lock().expect("in-flight task lock");
        let now = Instant::now();
        entries.values().map(|deadline| deadline.saturating_duration_since(now)).max()
    }
}

pub(crate) struct InFlightGuard {
    registry: InFlightTasks,
    judgement_id: Uuid,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.registry.entries.lock().expect("in-flight task lock").remove(&self.judgement_id);
    }
}

/// Transport- and protocol-level failures of the worker's AMQP sessions.
/// Every variant is logged by the session loops and triggers a reconnect;
/// none of them are Judge task failures, which travel as [`TaskFailure`]
/// payloads routed through the broker instead.
#[derive(Debug, thiserror::Error)]
pub enum RabbitWorkerError {
    #[error("RabbitMQ {0} timed out")]
    Timeout(&'static str),
    #[error(transparent)]
    Amqp(#[from] lapin::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Contract(#[from] project_balloon_contracts::ContractError),
    #[error("RabbitMQ cancelled the Judge task consumer")]
    ConsumerCancelled,
    #[error("timed out draining in-flight Judge tasks")]
    DrainTimeout,
    #[error("Judge task execution panicked: {0}")]
    TaskPanicked(String),
    #[error("RabbitMQ rejected or returned the Worker {0}")]
    Rejected(&'static str),
    #[error("Judge handler returned mismatched immutable identifiers")]
    MismatchedResultIdentifiers,
}

#[async_trait]
pub trait JudgeTaskHandler: Send + Sync {
    async fn handle(&self, task: &JudgeTask, retry_count: u32) -> Result<JudgeResult, TaskFailure>;
}

pub struct RabbitJudgeWorker {
    uri: String,
    task_queue: String,
    worker_id: String,
    prefetch: u16,
    request_timeout: Duration,
    reconnect_delay: Duration,
    max_task_cases: u32,
    in_flight: InFlightTasks,
    handler: Arc<dyn JudgeTaskHandler>,
    activity: WorkerActivity,
    health: Option<HealthState>,
}

pub struct RabbitJudgeWorkerConfig {
    pub uri: String,
    pub task_queue: String,
    pub worker_id: String,
    pub prefetch: u16,
    pub request_timeout: Duration,
    pub reconnect_delay: Duration,
    /// Upper bound on judged cases per task used for the per-task wall-clock
    /// deadline (the task contract does not carry the case count).
    pub max_task_cases: u32,
    /// Registry of in-flight judgements, shared with the orphan sweeper.
    pub in_flight: InFlightTasks,
    pub health: Option<HealthState>,
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
            max_task_cases: config.max_task_cases,
            in_flight: config.in_flight,
            handler,
            activity,
            health: config.health,
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
                if reason.to_string().contains("PRECONDITION_FAILED") {
                    error!(
                        "RabbitMQ topology mismatch: an existing queue or exchange was declared \
                         with different arguments. Align the broker topology with the API \
                         (or delete and let it be re-declared) before the worker can start."
                    );
                }
                if let Some(health) = &self.health {
                    health.record_session_failed(reason.to_string());
                }
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

    async fn consume_session(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), RabbitWorkerError> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| RabbitWorkerError::Timeout("worker connection"))??;
        let channel = connection.create_channel().await?;
        verify_topology(&channel, &self.task_queue).await?;
        channel.confirm_select(ConfirmSelectOptions::default()).await?;
        channel.basic_qos(self.prefetch, BasicQosOptions::default()).await?;
        let mut consumer = channel
            .basic_consume(
                self.task_queue.clone().into(),
                format!("project-balloon-worker-{}", self.worker_id).into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;
        if let Some(health) = &self.health {
            health.record_session_started();
        }
        let context = Arc::new(DeliveryContext {
            channel: channel.clone(),
            request_timeout: self.request_timeout,
            handler: self.handler.clone(),
            activity: self.activity.clone(),
            task_queue: self.task_queue.clone(),
            worker_id: self.worker_id.clone(),
            max_task_cases: self.max_task_cases,
            in_flight: self.in_flight.clone(),
        });
        let mut in_flight: JoinSet<DeliveryOutcome> = JoinSet::new();
        loop {
            tokio::select! {
                delivery = consumer.next(), if in_flight.len() < usize::from(self.prefetch) => {
                    let Some(delivery) = delivery else {
                        return self.end_session(&mut in_flight, RabbitWorkerError::ConsumerCancelled).await;
                    };
                    let delivery = match delivery {
                        Ok(delivery) => delivery,
                        Err(error) => {
                            return self.end_session(&mut in_flight, RabbitWorkerError::Amqp(error)).await;
                        }
                    };
                    let context = context.clone();
                    in_flight.spawn(async move { process_delivery(&context, &delivery).await });
                }
                joined = in_flight.join_next(), if !in_flight.is_empty() => {
                    let Some(joined) = joined else { continue };
                    match joined {
                        Ok(DeliveryOutcome::Contained) => {}
                        Ok(DeliveryOutcome::SessionFailed(error)) => {
                            error!(%error, "Judge task delivery failed at the protocol level");
                            return self.end_session(&mut in_flight, error).await;
                        }
                        Err(join_error) => {
                            // A panicked delivery can no longer ack or nack, so
                            // its slot can only be freed by ending the session
                            // (the broker then requeues it) after the healthy
                            // siblings finish.
                            let error = RabbitWorkerError::TaskPanicked(join_error.to_string());
                            error!(%error, "Judge task delivery panicked");
                            return self.end_session(&mut in_flight, error).await;
                        }
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        info!(active_tasks = in_flight.len(), "stopped accepting Judge tasks; draining in-flight work");
                        return match timeout(self.drain_bound(), drain_in_flight(&mut in_flight)).await {
                            Err(_) => Err(RabbitWorkerError::DrainTimeout),
                            Ok(result) => result,
                        };
                    }
                }
            }
        }
    }

    /// Bounded drain window: every in-flight judgement self-terminates at its
    /// own wall-clock deadline, so waiting out the longest remaining deadline
    /// plus slack for the confirmed publish and ack can never hang the session.
    fn drain_bound(&self) -> Duration {
        let publish_slack = self.request_timeout.saturating_mul(2) + Duration::from_secs(5);
        self.in_flight
            .max_remaining_deadline()
            .map_or(Duration::from_secs(1), |remaining| remaining + publish_slack)
    }

    /// Ends a failed session: in-flight judgements keep running until they
    /// finish (or hit their own deadlines) so their sandbox cleanup always
    /// executes, and only then does the session report its protocol failure.
    async fn end_session(
        &self,
        in_flight: &mut JoinSet<DeliveryOutcome>,
        reason: RabbitWorkerError,
    ) -> Result<(), RabbitWorkerError> {
        info!(
            active_tasks = in_flight.len(),
            reason = %reason,
            "Judge task session failed; draining in-flight work before returning"
        );
        match timeout(self.drain_bound(), drain_in_flight(in_flight)).await {
            Err(_) => Err(RabbitWorkerError::DrainTimeout),
            Ok(Err(drain_error)) => {
                warn!(%drain_error, "in-flight Judge task failed during the session drain");
                Err(reason)
            }
            Ok(Ok(())) => Err(reason),
        }
    }
}

async fn drain_in_flight(
    in_flight: &mut JoinSet<DeliveryOutcome>,
) -> Result<(), RabbitWorkerError> {
    let mut first_error = None;
    while let Some(joined) = in_flight.join_next().await {
        let outcome = joined.map_err(|error| RabbitWorkerError::TaskPanicked(error.to_string()))?;
        if let DeliveryOutcome::SessionFailed(error) = outcome
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

async fn verify_topology(channel: &Channel, task_queue: &str) -> Result<(), RabbitWorkerError> {
    const TASK_ROUTING_KEY: &str = "task";
    const RETRY_ROUTING_KEY: &str = "retry";

    // Declare instead of passively checking. A passive declare only verifies
    // that a name exists, so a queue rebuilt with wrong dead-letter arguments
    // silently swallowed every retry nack. A non-passive declare with the full
    // argument set fails with PRECONDITION_FAILED whenever an existing queue
    // or exchange disagrees, turning topology drift into a loud session
    // failure (surfaced on the health endpoint) instead of dropped messages.
    // The arguments here must mirror the API's judge_dispatch topology::declare.
    const RETRY_TTL_MILLISECONDS: i32 = 10_000;
    const HEARTBEAT_TTL_MILLISECONDS: i32 = 60_000;
    const HEARTBEAT_MAX_LENGTH: i32 = 10_000;

    let durable_exchange =
        ExchangeDeclareOptions { durable: true, ..ExchangeDeclareOptions::default() };
    for exchange in [
        JUDGE_TASKS_EXCHANGE,
        JUDGE_RETRY_EXCHANGE,
        JUDGE_RESULTS_EXCHANGE,
        JUDGE_RESULTS_RETRY_EXCHANGE,
        JUDGE_DEAD_EXCHANGE,
        JUDGE_HEARTBEATS_EXCHANGE,
    ] {
        channel
            .exchange_declare(
                exchange.into(),
                ExchangeKind::Direct,
                durable_exchange,
                FieldTable::default(),
            )
            .await?;
    }

    let durable_queue = QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() };
    channel
        .queue_declare(
            task_queue.into(),
            durable_queue,
            dead_letter_arguments(JUDGE_RETRY_EXCHANGE, RETRY_ROUTING_KEY),
        )
        .await?;
    let mut retry_arguments = dead_letter_arguments(JUDGE_TASKS_EXCHANGE, TASK_ROUTING_KEY);
    retry_arguments
        .insert(ShortString::from("x-message-ttl"), AMQPValue::LongInt(RETRY_TTL_MILLISECONDS));
    channel.queue_declare(JUDGE_RETRY_QUEUE.into(), durable_queue, retry_arguments).await?;
    channel.queue_declare(JUDGE_DEAD_QUEUE.into(), durable_queue, FieldTable::default()).await?;
    channel
        .queue_declare(
            JUDGE_RESULTS_QUEUE.into(),
            durable_queue,
            dead_letter_arguments(JUDGE_DEAD_EXCHANGE, JUDGE_DEAD_ROUTING_KEY),
        )
        .await?;
    let mut results_retry_arguments =
        dead_letter_arguments(JUDGE_RESULTS_EXCHANGE, JUDGE_RESULT_ROUTING_KEY);
    results_retry_arguments
        .insert(ShortString::from("x-message-ttl"), AMQPValue::LongInt(RETRY_TTL_MILLISECONDS));
    channel
        .queue_declare(JUDGE_RESULTS_RETRY_QUEUE.into(), durable_queue, results_retry_arguments)
        .await?;
    let mut heartbeat_arguments = FieldTable::default();
    heartbeat_arguments
        .insert(ShortString::from("x-message-ttl"), AMQPValue::LongInt(HEARTBEAT_TTL_MILLISECONDS));
    heartbeat_arguments
        .insert(ShortString::from("x-max-length"), AMQPValue::LongInt(HEARTBEAT_MAX_LENGTH));
    heartbeat_arguments.insert(
        ShortString::from("x-overflow"),
        AMQPValue::LongString(LongString::from("drop-head")),
    );
    channel
        .queue_declare(JUDGE_HEARTBEATS_QUEUE.into(), durable_queue, heartbeat_arguments)
        .await?;

    for (queue, exchange, routing_key) in [
        (task_queue, JUDGE_TASKS_EXCHANGE, TASK_ROUTING_KEY),
        (JUDGE_RETRY_QUEUE, JUDGE_RETRY_EXCHANGE, RETRY_ROUTING_KEY),
        (JUDGE_DEAD_QUEUE, JUDGE_DEAD_EXCHANGE, JUDGE_DEAD_ROUTING_KEY),
        (JUDGE_RESULTS_QUEUE, JUDGE_RESULTS_EXCHANGE, JUDGE_RESULT_ROUTING_KEY),
        (JUDGE_RESULTS_RETRY_QUEUE, JUDGE_RESULTS_RETRY_EXCHANGE, RETRY_ROUTING_KEY),
        (JUDGE_HEARTBEATS_QUEUE, JUDGE_HEARTBEATS_EXCHANGE, JUDGE_HEARTBEAT_ROUTING_KEY),
    ] {
        channel
            .queue_bind(
                queue.into(),
                exchange.into(),
                routing_key.into(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;
    }
    Ok(())
}

fn dead_letter_arguments(exchange: &str, routing_key: &str) -> FieldTable {
    let mut arguments = FieldTable::default();
    arguments.insert(
        ShortString::from("x-dead-letter-exchange"),
        AMQPValue::LongString(LongString::from(exchange)),
    );
    arguments.insert(
        ShortString::from("x-dead-letter-routing-key"),
        AMQPValue::LongString(LongString::from(routing_key)),
    );
    arguments
}

/// Everything one delivery's processing needs; built once per session and
/// cloned (cheaply) into every spawned delivery task.
struct DeliveryContext {
    channel: Channel,
    request_timeout: Duration,
    handler: Arc<dyn JudgeTaskHandler>,
    activity: WorkerActivity,
    task_queue: String,
    worker_id: String,
    max_task_cases: u32,
    in_flight: InFlightTasks,
}

/// Outcome of one delivery, distinguishing contained per-delivery failures
/// (which must never tear down the session and strand sibling judgements)
/// from protocol-level failures that end the session.
#[derive(Debug)]
enum DeliveryOutcome {
    /// Acked, dead-lettered, or handed back to the broker for retry.
    Contained,
    /// The broker session itself is broken; the caller must drain in-flight
    /// work and end the session.
    SessionFailed(RabbitWorkerError),
}

async fn process_delivery(context: &DeliveryContext, delivery: &Delivery) -> DeliveryOutcome {
    let task = match serde_json::from_slice::<JudgeTask>(&delivery.data) {
        Ok(task) => task,
        Err(error) => {
            return dead_letter_and_ack(
                context,
                delivery,
                &format!("malformed JudgeTask: {error}"),
            )
            .await;
        }
    };
    if let Err(error) = task.validate() {
        return dead_letter_and_ack(context, delivery, &format!("invalid JudgeTask: {error}"))
            .await;
    }
    if let Some(message_id) = delivery.properties.message_id().as_ref()
        && uuid::Uuid::parse_str(message_id.as_str()).ok() != Some(task.judgement_id)
    {
        return dead_letter_and_ack(
            context,
            delivery,
            "AMQP message ID does not match judgementId",
        )
        .await;
    }

    let retry_count = retry_count(delivery, &context.task_queue);
    let deadline = task_deadline(&task, context.max_task_cases);
    let _in_flight_guard = context.in_flight.guard(task.judgement_id, Instant::now() + deadline);
    let _activity_guard = context.activity.begin_task();
    // The handler runs under the task's wall-clock deadline, so a wedged
    // sandbox or artifact call can hold its prefetch slot for at most one
    // deadline instead of forever.
    match timeout(deadline, context.handler.handle(&task, retry_count)).await {
        Ok(Ok(result)) => {
            if let Err(reason) = validate_handler_result(&task, &result) {
                return dead_letter_and_ack(context, delivery, &reason.to_string()).await;
            }
            publish_result_and_ack(context, delivery, result).await
        }
        Ok(Err(failure)) if failure.kind == TaskFailureKind::Dead => {
            dead_letter_and_ack(context, delivery, &failure.reason).await
        }
        Ok(Err(failure)) => {
            warn!(
                judgement_id = %task.judgement_id,
                reason = %failure.reason,
                "Judge task scheduled for broker retry"
            );
            nack_for_retry(delivery).await
        }
        Err(_elapsed) => {
            // The handler missed its deadline and was cancelled. Its sandbox
            // cleanup is best-effort from here: the redelivery reaps the
            // abandoned container through the name-conflict path and pre-cleans
            // the job directory, and the orphan sweeper reclaims anything left
            // over. Route through the retry budget so there is no crash loop,
            // no lost slot, and no silent hang.
            let reason = format!(
                "Judge task exceeded its wall-clock deadline of {deadline:?}; the handler was cancelled"
            );
            match deadline_outcome(retry_count) {
                DeadlineOutcome::Retry => {
                    warn!(
                        judgement_id = %task.judgement_id,
                        reason,
                        "Judge task missed its deadline; scheduled for broker retry"
                    );
                    nack_for_retry(delivery).await
                }
                DeadlineOutcome::SystemError => {
                    warn!(
                        judgement_id = %task.judgement_id,
                        reason,
                        "Judge task missed its deadline with the retry budget exhausted; reporting SystemError"
                    );
                    publish_result_and_ack(
                        context,
                        delivery,
                        deadline_system_error(&task, &context.worker_id, &reason),
                    )
                    .await
                }
            }
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
fn retry_count(delivery: &Delivery, task_queue: &str) -> u32 {
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
                    Some(AMQPValue::LongString(queue)) => queue.as_bytes() == task_queue.as_bytes(),
                    Some(AMQPValue::ShortString(queue)) => queue.as_str() == task_queue,
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

fn validate_handler_result(
    task: &JudgeTask,
    result: &JudgeResult,
) -> Result<(), RabbitWorkerError> {
    result.validate()?;
    if result.judgement_id != task.judgement_id
        || result.submission_id != task.submission_id
        || result.message_id != task.judgement_id
    {
        return Err(RabbitWorkerError::MismatchedResultIdentifiers);
    }
    Ok(())
}

async fn dead_letter_and_ack(
    context: &DeliveryContext,
    delivery: &Delivery,
    reason: &str,
) -> DeliveryOutcome {
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
        &context.channel,
        context.request_timeout,
        JUDGE_DEAD_EXCHANGE,
        JUDGE_DEAD_ROUTING_KEY,
        message_id,
        &delivery.data,
        headers,
    )
    .await
    {
        warn!(
            %safe_reason,
            %error,
            "Judge task dead-letter publication failed; scheduling the task for broker retry"
        );
        return nack_for_retry(delivery).await;
    }
    // The dead-letter copy is broker-confirmed; an ack failure must not
    // propagate or the redelivered task would be dead-lettered forever.
    if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
        warn!(
            %safe_reason,
            %error,
            "Judge task moved to dead-letter queue but ack failed; it may be redelivered and dead-lettered again"
        );
    } else {
        warn!(%safe_reason, "Judge task moved to dead-letter queue");
    }
    DeliveryOutcome::Contained
}

/// Publishes the confirmed persistent result and only then acks the delivery,
/// preserving the ack-after-confirmed-publish ordering.
async fn publish_result_and_ack(
    context: &DeliveryContext,
    delivery: &Delivery,
    result: JudgeResult,
) -> DeliveryOutcome {
    let payload = match serde_json::to_vec(&result) {
        Ok(payload) => payload,
        Err(error) => {
            // A result we cannot serialize would fail on every retry; move the
            // delivery to the dead-letter queue with the reason instead.
            return dead_letter_and_ack(
                context,
                delivery,
                &format!("unserializable JudgeResult: {error}"),
            )
            .await;
        }
    };
    if let Err(reason) = publish_persistent(
        &context.channel,
        context.request_timeout,
        JUDGE_RESULTS_EXCHANGE,
        JUDGE_RESULT_ROUTING_KEY,
        result.message_id,
        &payload,
        FieldTable::default(),
    )
    .await
    {
        warn!(
            judgement_id = %result.judgement_id,
            reason = %reason,
            "Judge result publication failed; scheduling the task for broker retry"
        );
        return nack_for_retry(delivery).await;
    }
    // The result is already broker-confirmed; treat the task as processed
    // instead of propagating the ack failure, which would tear down the
    // shared connection and redeliver live tasks.
    if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
        warn!(
            judgement_id = %result.judgement_id,
            error = %error,
            "Judge result published and confirmed; ack failed, message may be redelivered (idempotent downstream)"
        );
    }
    info!(
        judgement_id = %result.judgement_id,
        verdict = result.verdict.as_str(),
        "Judge result confirmed; task acknowledged"
    );
    DeliveryOutcome::Contained
}

/// Nacks the delivery for a broker retry. A failing nack means the channel
/// itself is broken — a protocol failure that must end the session.
async fn nack_for_retry(delivery: &Delivery) -> DeliveryOutcome {
    match delivery.nack(retry_nack_options()).await {
        Ok(_) => DeliveryOutcome::Contained,
        Err(error) => DeliveryOutcome::SessionFailed(RabbitWorkerError::Amqp(error)),
    }
}

fn retry_nack_options() -> BasicNackOptions {
    BasicNackOptions { multiple: false, requeue: false }
}

async fn publish_persistent(
    channel: &Channel,
    request_timeout: Duration,
    exchange: &str,
    routing_key: &str,
    message_id: uuid::Uuid,
    payload: &[u8],
    mut headers: FieldTable,
) -> Result<(), RabbitWorkerError> {
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
    .map_err(|_| RabbitWorkerError::Timeout("worker publish"))??;
    let confirmation = timeout(request_timeout, confirm)
        .await
        .map_err(|_| RabbitWorkerError::Timeout("worker confirm"))??;
    if confirmation.is_ack() && confirmation.take_message().is_none() {
        Ok(())
    } else {
        Err(RabbitWorkerError::Rejected("publication"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lapin::{
        BasicProperties,
        message::Delivery,
        types::{AMQPValue, FieldArray, FieldTable, LongString, ShortString},
    };
    use project_balloon_contracts::{
        JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeRunResult, JudgeTask, JudgeVerdict,
    };
    use time::OffsetDateTime;
    use uuid::Uuid;

    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::task::JoinSet;

    use crate::heartbeat::WorkerActivity;
    use crate::rabbit::{
        DeliveryOutcome, InFlightTasks, JudgeTaskHandler, RabbitJudgeWorker,
        RabbitJudgeWorkerConfig, RabbitWorkerError, TaskFailure, retry_budget_exhausted,
        retry_count, retry_nack_options, task_deadline, validate_handler_result,
    };

    #[test]
    fn retry_rejection_enters_dead_letter_flow() {
        let options = retry_nack_options();
        assert!(!options.requeue);
    }

    #[test]
    fn retry_count_only_sums_deaths_from_the_active_task_queue() {
        let mut task_death = FieldTable::default();
        task_death.insert(
            ShortString::from("queue"),
            AMQPValue::LongString(LongString::from("custom.tasks")),
        );
        task_death.insert(ShortString::from("count"), AMQPValue::LongInt(2));
        let mut retry_death = FieldTable::default();
        retry_death.insert(
            ShortString::from("queue"),
            AMQPValue::LongString(LongString::from("judge.retry")),
        );
        retry_death.insert(ShortString::from("count"), AMQPValue::LongLongInt(7));
        let headers = FieldTable::from(BTreeMap::from([(
            ShortString::from("x-death"),
            AMQPValue::FieldArray(FieldArray::from(vec![
                AMQPValue::FieldTable(task_death),
                AMQPValue::FieldTable(retry_death),
            ])),
        )]));
        let mut delivery =
            Delivery::mock(1, ShortString::from(""), ShortString::from("task"), false, Vec::new());
        delivery.properties = BasicProperties::default().with_headers(headers);

        assert_eq!(retry_count(&delivery, "custom.tasks"), 2);
        assert_eq!(retry_count(&delivery, "judge.tasks"), 0);
    }

    #[test]
    fn handler_result_must_keep_task_identity() {
        let now = OffsetDateTime::now_utc();
        let judgement_id = Uuid::new_v4();
        let task = JudgeTask {
            schema_version: 1,
            judgement_id,
            submission_id: 42,
            problem_id: 7,
            testdata_version: 1,
            testdata_object_key: "problems/7/v1.zip".into(),
            testdata_sha256: "a".repeat(64),
            source_object_key: "submissions/42/main.cpp".into(),
            source_sha256: "b".repeat(64),
            language: "cpp".into(),
            time_limit_ms: 1000,
            memory_limit_mb: 256,
            output_limit_kb: 64,
            language_multiplier: 1.0,
            judge_mode: Default::default(),
            interactor_object_key: None,
            interactor_sha256: None,
        };
        let result = JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: judgement_id,
            judgement_id: Uuid::new_v4(),
            submission_id: 42,
            worker_id: "worker-1".into(),
            verdict: JudgeVerdict::CompileError,
            total_time_ms: 0,
            peak_memory_kb: 0,
            compile_log: None,
            started_at: now,
            completed_at: now,
            runs: Vec::<JudgeRunResult>::new(),
        };

        assert!(validate_handler_result(&task, &result).is_err());
    }

    fn sample_task(judgement_id: Uuid) -> JudgeTask {
        JudgeTask {
            schema_version: 1,
            judgement_id,
            submission_id: 42,
            problem_id: 7,
            testdata_version: 1,
            testdata_object_key: "problems/7/v1.zip".into(),
            testdata_sha256: "a".repeat(64),
            source_object_key: "submissions/42/main.cpp".into(),
            source_sha256: "b".repeat(64),
            language: "cpp".into(),
            time_limit_ms: 1000,
            memory_limit_mb: 256,
            output_limit_kb: 64,
            language_multiplier: 1.0,
            judge_mode: Default::default(),
            interactor_object_key: None,
            interactor_sha256: None,
        }
    }

    fn result_for(task: &JudgeTask, judgement_id: Uuid, submission_id: i64) -> JudgeResult {
        let now = OffsetDateTime::now_utc();
        JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: task.judgement_id,
            judgement_id,
            submission_id,
            worker_id: "worker-1".into(),
            verdict: JudgeVerdict::CompileError,
            total_time_ms: 1,
            peak_memory_kb: 1,
            compile_log: Some("cc: error".into()),
            started_at: now,
            completed_at: now,
            runs: Vec::<JudgeRunResult>::new(),
        }
    }

    #[test]
    fn handler_result_with_matching_identity_is_accepted() {
        let judgement_id = Uuid::new_v4();
        let task = sample_task(judgement_id);
        assert!(validate_handler_result(&task, &result_for(&task, judgement_id, 42)).is_ok());
    }

    #[test]
    fn handler_result_must_keep_the_submission_identity() {
        let judgement_id = Uuid::new_v4();
        let task = sample_task(judgement_id);
        assert!(validate_handler_result(&task, &result_for(&task, judgement_id, 43)).is_err());
    }

    #[test]
    fn task_deadline_mirrors_the_sandbox_stage_limits() {
        let task = sample_task(Uuid::new_v4());
        // 30 s compile + 300 s margin + 1000 cases x max(3 x 1 s, 1 s).
        assert_eq!(task_deadline(&task, 1_000), Duration::from_secs(330 + 3_000));
        // The multiplier feeds the per-case wall limit.
        let mut slow = sample_task(Uuid::new_v4());
        slow.language_multiplier = 2.0;
        // 2 x effective limit => 6 s per case.
        assert_eq!(task_deadline(&slow, 10), Duration::from_secs(330 + 60));
        // Output-only tasks never enter a container: the margin bounds them.
        let mut output = sample_task(Uuid::new_v4());
        output.judge_mode = project_balloon_contracts::JudgeMode::OutputOnly;
        assert_eq!(task_deadline(&output, 1_000), Duration::from_secs(300));
    }

    #[test]
    fn deadline_expiry_routes_through_the_retry_budget() {
        for retry_count in 0..8 {
            assert_eq!(
                super::deadline_outcome(retry_count),
                super::DeadlineOutcome::Retry,
                "retry_count {retry_count} must stay within the budget"
            );
            assert!(!retry_budget_exhausted(retry_count));
        }
        for retry_count in 8..=12 {
            assert_eq!(
                super::deadline_outcome(retry_count),
                super::DeadlineOutcome::SystemError,
                "retry_count {retry_count} must be terminal"
            );
            assert!(retry_budget_exhausted(retry_count));
        }
    }

    #[test]
    fn deadline_system_error_is_contract_valid_and_identity_safe() {
        let task = sample_task(Uuid::new_v4());
        let result = super::deadline_system_error(
            &task,
            "worker-under-test",
            "Judge task exceeded its wall-clock deadline of 55m; the handler was cancelled",
        );
        result.validate().expect("the deadline verdict must be contract-valid");
        assert_eq!(result.verdict, JudgeVerdict::SystemError);
        assert_eq!(result.judgement_id, task.judgement_id);
        assert_eq!(result.message_id, task.judgement_id);
        assert_eq!(result.submission_id, task.submission_id);
        assert!(result.runs.is_empty());
    }

    #[test]
    fn in_flight_registry_tracks_guards_and_remaining_deadlines() {
        let registry = InFlightTasks::new();
        assert_eq!(registry.max_remaining_deadline(), None);
        assert!(registry.judgement_ids().is_empty());

        let first =
            registry.guard(Uuid::new_v4(), std::time::Instant::now() + Duration::from_secs(5));
        let second =
            registry.guard(Uuid::new_v4(), std::time::Instant::now() + Duration::from_secs(60));
        assert_eq!(registry.judgement_ids().len(), 2);
        let remaining = registry.max_remaining_deadline().expect("deadlines");
        assert!(remaining > Duration::from_secs(30), "the longest deadline must dominate");

        drop(first);
        assert_eq!(registry.judgement_ids().len(), 1);
        drop(second);
        assert_eq!(registry.max_remaining_deadline(), None);
    }

    fn test_worker(in_flight: InFlightTasks) -> RabbitJudgeWorker {
        RabbitJudgeWorker::new(
            RabbitJudgeWorkerConfig {
                uri: "amqp://127.0.0.1:5672/%2f".to_owned(),
                task_queue: "judge.tasks".to_owned(),
                worker_id: "deadline-test-worker".to_owned(),
                prefetch: 2,
                request_timeout: Duration::from_secs(1),
                reconnect_delay: Duration::from_millis(1),
                max_task_cases: 100,
                in_flight,
                health: None,
            },
            Arc::new(StubHandler),
            WorkerActivity::new(2),
        )
    }

    #[test]
    fn drain_bound_derives_from_in_flight_deadlines() {
        let registry = InFlightTasks::new();
        let worker = test_worker(registry.clone());
        // Nothing in flight: the drain finishes immediately.
        assert_eq!(worker.drain_bound(), Duration::from_secs(1));

        let _guard =
            registry.guard(Uuid::new_v4(), std::time::Instant::now() + Duration::from_secs(30));
        let bound = worker.drain_bound();
        assert!(bound >= Duration::from_secs(30), "the deadline must drive the bound");
        assert!(bound <= Duration::from_secs(30 + 10), "the bound must stay bounded");
    }

    struct StubHandler;

    #[async_trait]
    impl JudgeTaskHandler for StubHandler {
        async fn handle(
            &self,
            task: &JudgeTask,
            _retry_count: u32,
        ) -> Result<JudgeResult, TaskFailure> {
            Ok(result_for(&sample_task(task.judgement_id), task.judgement_id, task.submission_id))
        }
    }

    #[tokio::test]
    async fn session_failure_drains_sibling_judgements_before_returning() {
        let registry = InFlightTasks::new();
        let worker = test_worker(registry.clone());
        let mut in_flight: JoinSet<DeliveryOutcome> = JoinSet::new();

        // A healthy sibling that needs a moment to finish its (simulated)
        // sandbox work and cleanup.
        let completed = Arc::new(AtomicUsize::new(0));
        let flag = completed.clone();
        in_flight.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            flag.fetch_add(1, Ordering::SeqCst);
            DeliveryOutcome::Contained
        });

        // The protocol failure must not abort the sibling: end_session drains
        // before returning.
        let error = worker
            .end_session(&mut in_flight, RabbitWorkerError::ConsumerCancelled)
            .await
            .expect_err("a failed session must surface its reason");
        assert!(matches!(error, RabbitWorkerError::ConsumerCancelled));
        assert_eq!(completed.load(Ordering::SeqCst), 1, "the sibling must finish first");
        assert!(registry.judgement_ids().is_empty());
    }

    #[tokio::test]
    async fn failing_deliveries_are_contained_and_siblings_complete() {
        // Model of the restructured consume_session loop: each delivery runs as
        // a JoinSet task producing a DeliveryOutcome. Per-delivery failures are
        // contained; only a SessionFailed outcome ends the session, and the
        // drain still lets healthy siblings finish.
        let mut in_flight: JoinSet<DeliveryOutcome> = JoinSet::new();
        let completed = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let flag = completed.clone();
            in_flight.spawn(async move {
                // Simulates process_delivery containing a handler/publish
                // failure: the outcome stays Contained, not session-fatal.
                flag.fetch_add(1, Ordering::SeqCst);
                DeliveryOutcome::Contained
            });
        }
        let mut session_error = None;
        while let Some(joined) = in_flight.join_next().await {
            match joined {
                Ok(DeliveryOutcome::Contained) => {}
                Ok(DeliveryOutcome::SessionFailed(error)) => {
                    session_error.get_or_insert(error);
                }
                Err(join_error) => {
                    session_error
                        .get_or_insert(RabbitWorkerError::TaskPanicked(join_error.to_string()));
                }
            }
        }
        assert!(session_error.is_none(), "contained failures must not end the session");
        assert_eq!(completed.load(Ordering::SeqCst), 3, "every sibling ran to completion");
    }

    #[tokio::test]
    async fn drain_in_flight_collects_protocol_failures_after_all_tasks_settle() {
        let mut in_flight: JoinSet<DeliveryOutcome> = JoinSet::new();
        in_flight.spawn(async { DeliveryOutcome::Contained });
        in_flight.spawn(async {
            DeliveryOutcome::SessionFailed(RabbitWorkerError::Rejected("publication"))
        });
        let error = super::drain_in_flight(&mut in_flight).await.expect_err("protocol failure");
        assert!(matches!(error, RabbitWorkerError::Rejected("publication")));
        assert!(in_flight.is_empty(), "the drain must consume every task");
    }
}
