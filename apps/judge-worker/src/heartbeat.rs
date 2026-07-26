use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use lapin::{
    BasicProperties, Connection, ConnectionProperties, ExchangeKind,
    options::{BasicPublishOptions, ConfirmSelectOptions, ExchangeDeclareOptions},
    types::FieldTable,
};
use project_balloon_contracts::{
    JUDGE_HEARTBEAT_ROUTING_KEY, JUDGE_HEARTBEATS_EXCHANGE, WORKER_HEARTBEAT_SCHEMA_VERSION,
    WorkerHeartbeat,
};
use time::OffsetDateTime;
use tokio::{sync::watch, time::timeout};
use tracing::{error, info};
use uuid::Uuid;

struct WorkerActivityState {
    capacity: u16,
    active_tasks: AtomicU16,
}

#[derive(Clone)]
pub struct WorkerActivity(Arc<WorkerActivityState>);

impl WorkerActivity {
    #[must_use]
    pub fn new(capacity: u16) -> Self {
        Self(Arc::new(WorkerActivityState { capacity, active_tasks: AtomicU16::new(0) }))
    }

    #[must_use]
    pub fn capacity(&self) -> u16 {
        self.0.capacity
    }

    #[must_use]
    pub fn active_tasks(&self) -> u16 {
        self.0.active_tasks.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn begin_task(&self) -> TaskActivityGuard {
        self.0.active_tasks.fetch_add(1, Ordering::Relaxed);
        TaskActivityGuard { activity: self.clone() }
    }
}

impl Default for WorkerActivity {
    fn default() -> Self {
        Self::new(1)
    }
}

pub struct TaskActivityGuard {
    activity: WorkerActivity,
}

impl Drop for TaskActivityGuard {
    fn drop(&mut self) {
        self.activity.0.active_tasks.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct WorkerHeartbeatPublisher {
    uri: String,
    worker_id: String,
    instance_id: Uuid,
    started_at: OffsetDateTime,
    interval: Duration,
    request_timeout: Duration,
    reconnect_delay: Duration,
    activity: WorkerActivity,
    runtime_versions: BTreeMap<String, String>,
    sandbox_runtime: Option<String>,
}

pub struct WorkerHeartbeatPublisherConfig {
    pub uri: String,
    pub worker_id: String,
    pub interval: Duration,
    pub request_timeout: Duration,
    pub reconnect_delay: Duration,
    pub runtime_versions: BTreeMap<String, String>,
    pub sandbox_runtime: Option<String>,
}

impl WorkerHeartbeatPublisher {
    #[must_use]
    pub fn new(config: WorkerHeartbeatPublisherConfig, activity: WorkerActivity) -> Self {
        Self {
            uri: config.uri,
            worker_id: config.worker_id,
            instance_id: Uuid::new_v4(),
            started_at: OffsetDateTime::now_utc(),
            interval: config.interval,
            request_timeout: config.request_timeout,
            reconnect_delay: config.reconnect_delay,
            activity,
            runtime_versions: config.runtime_versions,
            sandbox_runtime: config.sandbox_runtime,
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        info!(worker_id = %self.worker_id, "Worker heartbeat publisher started");
        loop {
            if *shutdown.borrow() {
                break;
            }
            if let Err(reason) = self.publish_session(shutdown.clone()).await {
                error!(%reason, "Worker heartbeat publisher session failed");
            }
            tokio::select! {
                () = tokio::time::sleep(self.reconnect_delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { break; }
                }
            }
        }
        info!(worker_id = %self.worker_id, "Worker heartbeat publisher stopped");
    }

    async fn publish_session(&self, mut shutdown: watch::Receiver<bool>) -> Result<(), String> {
        let connection = timeout(
            self.request_timeout,
            Connection::connect(&self.uri, ConnectionProperties::default()),
        )
        .await
        .map_err(|_| "RabbitMQ heartbeat publisher connection timed out".to_owned())?
        .map_err(|error| error.to_string())?;
        let channel = connection.create_channel().await.map_err(|error| error.to_string())?;
        channel
            .exchange_declare(
                JUDGE_HEARTBEATS_EXCHANGE.into(),
                ExchangeKind::Direct,
                ExchangeDeclareOptions {
                    passive: true,
                    durable: true,
                    ..ExchangeDeclareOptions::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|error| error.to_string())?;
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => self.publish(&channel).await?,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return Ok(()); }
                }
            }
        }
    }

    async fn publish(&self, channel: &lapin::Channel) -> Result<(), String> {
        let message_id = Uuid::new_v4();
        let heartbeat = WorkerHeartbeat {
            schema_version: WORKER_HEARTBEAT_SCHEMA_VERSION,
            message_id,
            worker_id: self.worker_id.clone(),
            instance_id: self.instance_id,
            started_at: self.started_at,
            occurred_at: OffsetDateTime::now_utc(),
            capacity: self.activity.capacity(),
            active_tasks: self.activity.active_tasks(),
            languages: vec![
                "c".to_owned(),
                "cpp".to_owned(),
                "java".to_owned(),
                "python".to_owned(),
            ],
            runtime_versions: self.runtime_versions.clone(),
            sandbox_runtime: self.sandbox_runtime.clone(),
        };
        heartbeat.validate().map_err(|error| error.to_string())?;
        let payload = serde_json::to_vec(&heartbeat).map_err(|error| error.to_string())?;
        let confirm = timeout(
            self.request_timeout,
            channel.basic_publish(
                JUDGE_HEARTBEATS_EXCHANGE.into(),
                JUDGE_HEARTBEAT_ROUTING_KEY.into(),
                BasicPublishOptions { mandatory: true, ..BasicPublishOptions::default() },
                &payload,
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_message_id(message_id.to_string().into()),
            ),
        )
        .await
        .map_err(|_| "RabbitMQ heartbeat publish timed out".to_owned())?
        .map_err(|error| error.to_string())?;
        let confirmation = timeout(self.request_timeout, confirm)
            .await
            .map_err(|_| "RabbitMQ heartbeat confirm timed out".to_owned())?
            .map_err(|error| error.to_string())?;
        if confirmation.is_ack() && confirmation.take_message().is_none() {
            Ok(())
        } else {
            Err("RabbitMQ rejected or returned the Worker heartbeat".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerActivity;

    #[test]
    fn task_guards_track_parallel_activity_and_release_on_drop() {
        let activity = WorkerActivity::new(3);
        let first = activity.begin_task();
        let second = activity.begin_task();
        assert_eq!(activity.capacity(), 3);
        assert_eq!(activity.active_tasks(), 2);
        drop(first);
        assert_eq!(activity.active_tasks(), 1);
        drop(second);
        assert_eq!(activity.active_tasks(), 0);
    }
}
