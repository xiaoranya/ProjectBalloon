use lapin::{
    Channel, ExchangeKind,
    options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::{AMQPValue, FieldTable, LongString, ShortString},
};
use project_balloon_contracts::{
    JUDGE_DEAD_EXCHANGE, JUDGE_DEAD_QUEUE, JUDGE_HEARTBEAT_ROUTING_KEY, JUDGE_HEARTBEATS_EXCHANGE,
    JUDGE_HEARTBEATS_QUEUE, JUDGE_RESULTS_EXCHANGE, JUDGE_RESULTS_QUEUE, JUDGE_RETRY_EXCHANGE,
    JUDGE_RETRY_QUEUE, JUDGE_TASKS_EXCHANGE, JUDGE_TASKS_QUEUE,
};

pub const TASKS_QUEUE: &str = JUDGE_TASKS_QUEUE;
pub const RETRY_QUEUE: &str = JUDGE_RETRY_QUEUE;
pub const DEAD_QUEUE: &str = JUDGE_DEAD_QUEUE;
pub const RESULTS_QUEUE: &str = JUDGE_RESULTS_QUEUE;
pub const TASKS_EXCHANGE: &str = JUDGE_TASKS_EXCHANGE;
pub const RETRY_EXCHANGE: &str = JUDGE_RETRY_EXCHANGE;
pub const DEAD_EXCHANGE: &str = JUDGE_DEAD_EXCHANGE;
pub const RESULTS_EXCHANGE: &str = JUDGE_RESULTS_EXCHANGE;
pub const TASK_ROUTING_KEY: &str = "task";
pub const RETRY_ROUTING_KEY: &str = "retry";
pub const DEAD_ROUTING_KEY: &str = "dead";
pub const RESULT_ROUTING_KEY: &str = "result";
const RETRY_TTL_MILLISECONDS: i32 = 10_000;
const HEARTBEAT_TTL_MILLISECONDS: i32 = 60_000;
const HEARTBEAT_MAX_LENGTH: i32 = 10_000;

pub async fn declare(channel: &Channel) -> Result<(), lapin::Error> {
    for exchange in
        [TASKS_EXCHANGE, RETRY_EXCHANGE, DEAD_EXCHANGE, RESULTS_EXCHANGE, JUDGE_HEARTBEATS_EXCHANGE]
    {
        channel
            .exchange_declare(
                exchange.into(),
                ExchangeKind::Direct,
                ExchangeDeclareOptions { durable: true, ..ExchangeDeclareOptions::default() },
                FieldTable::default(),
            )
            .await?;
    }

    let mut tasks_arguments = FieldTable::default();
    tasks_arguments.insert(
        ShortString::from("x-dead-letter-exchange"),
        AMQPValue::LongString(LongString::from(RETRY_EXCHANGE)),
    );
    tasks_arguments.insert(
        ShortString::from("x-dead-letter-routing-key"),
        AMQPValue::LongString(LongString::from(RETRY_ROUTING_KEY)),
    );
    channel
        .queue_declare(
            TASKS_QUEUE.into(),
            QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() },
            tasks_arguments,
        )
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
    // Heartbeats are ephemeral presence signals. Expiry while every API
    // consumer is offline is expected and must not pollute the task/result
    // dead-letter queue or make readiness report false judge failures.
    channel
        .queue_declare(
            JUDGE_HEARTBEATS_QUEUE.into(),
            QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() },
            heartbeat_arguments,
        )
        .await?;

    let mut retry_arguments = FieldTable::default();
    retry_arguments
        .insert(ShortString::from("x-message-ttl"), AMQPValue::LongInt(RETRY_TTL_MILLISECONDS));
    retry_arguments.insert(
        ShortString::from("x-dead-letter-exchange"),
        AMQPValue::LongString(LongString::from(TASKS_EXCHANGE)),
    );
    retry_arguments.insert(
        ShortString::from("x-dead-letter-routing-key"),
        AMQPValue::LongString(LongString::from(TASK_ROUTING_KEY)),
    );
    channel
        .queue_declare(
            RETRY_QUEUE.into(),
            QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() },
            retry_arguments,
        )
        .await?;
    for queue in [DEAD_QUEUE] {
        channel
            .queue_declare(
                queue.into(),
                QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() },
                FieldTable::default(),
            )
            .await?;
    }
    let mut results_arguments = FieldTable::default();
    results_arguments.insert(
        ShortString::from("x-dead-letter-exchange"),
        AMQPValue::LongString(LongString::from(DEAD_EXCHANGE)),
    );
    results_arguments.insert(
        ShortString::from("x-dead-letter-routing-key"),
        AMQPValue::LongString(LongString::from(DEAD_ROUTING_KEY)),
    );
    channel
        .queue_declare(
            RESULTS_QUEUE.into(),
            QueueDeclareOptions { durable: true, ..QueueDeclareOptions::default() },
            results_arguments,
        )
        .await?;

    for (queue, exchange, routing_key) in [
        (TASKS_QUEUE, TASKS_EXCHANGE, TASK_ROUTING_KEY),
        (RETRY_QUEUE, RETRY_EXCHANGE, RETRY_ROUTING_KEY),
        (DEAD_QUEUE, DEAD_EXCHANGE, DEAD_ROUTING_KEY),
        (RESULTS_QUEUE, RESULTS_EXCHANGE, RESULT_ROUTING_KEY),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_names_match_the_reviewed_java_contract() {
        assert_eq!(
            (TASKS_QUEUE, TASKS_EXCHANGE, TASK_ROUTING_KEY),
            ("judge.tasks", "judge.tasks.exchange", "task")
        );
        assert_eq!(
            (RETRY_QUEUE, RETRY_EXCHANGE, RETRY_ROUTING_KEY),
            ("judge.retry", "judge.retry.exchange", "retry")
        );
        assert_eq!(
            (DEAD_QUEUE, DEAD_EXCHANGE, DEAD_ROUTING_KEY),
            ("judge.dead", "judge.dead.exchange", "dead")
        );
        assert_eq!(RESULTS_QUEUE, "judge.results");
        assert_eq!(JUDGE_HEARTBEATS_QUEUE, "judge.heartbeats");
    }
}
