use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties,
    options::{BasicPublishOptions, ConfirmSelectOptions},
    types::{AMQPValue, FieldTable, LongString, ShortString},
};
use tokio::{sync::Mutex, time::timeout};
use uuid::Uuid;

use crate::features::judge_dispatch::{
    dispatcher::JudgeTaskPublisher, error::JudgeDispatchError, topology,
};

pub struct RabbitJudgeTaskPublisher {
    uri: String,
    timeout: Duration,
    channel: Mutex<Option<Channel>>,
}

pub struct RabbitJudgeProbe {
    pub queued_tasks: u32,
    pub queued_results: u32,
    pub dead_tasks: u32,
}

impl RabbitJudgeTaskPublisher {
    #[must_use]
    pub fn new(uri: String, timeout: Duration) -> Arc<Self> {
        Arc::new(Self { uri, timeout, channel: Mutex::new(None) })
    }

    async fn connect(&self) -> Result<Channel, JudgeDispatchError> {
        let connection =
            timeout(self.timeout, Connection::connect(&self.uri, ConnectionProperties::default()))
                .await
                .map_err(|_| JudgeDispatchError::Timeout("connection"))??;
        let channel = connection.create_channel().await?;
        topology::declare(&channel).await?;
        channel.confirm_select(ConfirmSelectOptions::default()).await?;
        Ok(channel)
    }

    pub async fn probe(&self) -> Result<RabbitJudgeProbe, JudgeDispatchError> {
        let mut guard = self.channel.lock().await;
        if guard.as_ref().is_none_or(|channel| !channel.status().connected()) {
            *guard = Some(self.connect().await?);
        }
        let channel = guard.as_ref().ok_or(JudgeDispatchError::ChannelUnavailable)?;
        let passive = lapin::options::QueueDeclareOptions {
            passive: true,
            durable: true,
            ..lapin::options::QueueDeclareOptions::default()
        };
        let tasks = channel
            .queue_declare(topology::TASKS_QUEUE.into(), passive, FieldTable::default())
            .await?;
        let dead = channel
            .queue_declare(topology::DEAD_QUEUE.into(), passive, FieldTable::default())
            .await?;
        let results = channel
            .queue_declare(topology::RESULTS_QUEUE.into(), passive, FieldTable::default())
            .await?;
        Ok(RabbitJudgeProbe {
            queued_tasks: tasks.message_count(),
            queued_results: results.message_count(),
            dead_tasks: dead.message_count(),
        })
    }

    async fn publish_on(
        &self,
        channel: &Channel,
        message_id: Uuid,
        payload: &[u8],
    ) -> Result<(), JudgeDispatchError> {
        let message_id = message_id.to_string();
        let mut headers = FieldTable::default();
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
            self.timeout,
            channel.basic_publish(
                topology::TASKS_EXCHANGE.into(),
                topology::TASK_ROUTING_KEY.into(),
                BasicPublishOptions { mandatory: true, ..BasicPublishOptions::default() },
                payload,
                properties,
            ),
        )
        .await
        .map_err(|_| JudgeDispatchError::Timeout("publish"))??;
        let confirmation = timeout(self.timeout, confirm)
            .await
            .map_err(|_| JudgeDispatchError::Timeout("publisher confirm"))??;
        let acknowledged = confirmation.is_ack();
        let routed = confirmation.take_message().is_none();
        if acknowledged && routed {
            Ok(())
        } else {
            Err(JudgeDispatchError::Rejected("Judge task"))
        }
    }
}

#[async_trait]
impl JudgeTaskPublisher for RabbitJudgeTaskPublisher {
    async fn publish(&self, message_id: Uuid, payload: &[u8]) -> Result<(), JudgeDispatchError> {
        let mut guard = self.channel.lock().await;
        if guard.as_ref().is_none_or(|channel| !channel.status().connected()) {
            *guard = Some(self.connect().await?);
        }
        let first = guard.as_ref().ok_or(JudgeDispatchError::ChannelUnavailable)?;
        if self.publish_on(first, message_id, payload).await.is_ok() {
            return Ok(());
        }
        *guard = Some(self.connect().await?);
        let retry = guard.as_ref().ok_or(JudgeDispatchError::ChannelUnavailable)?;
        self.publish_on(retry, message_id, payload).await
    }
}
