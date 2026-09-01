use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use bollard::Docker;
use lapin::{
    BasicProperties, Connection, ConnectionProperties, ExchangeKind,
    options::{
        BasicAckOptions, BasicGetOptions, BasicPublishOptions, ConfirmSelectOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    types::FieldTable,
};
use project_balloon_contracts::{
    JUDGE_HEARTBEAT_ROUTING_KEY, JUDGE_HEARTBEATS_EXCHANGE, JUDGE_RESULT_SCHEMA_VERSION,
    JUDGE_TASKS_EXCHANGE, JudgeResult, JudgeRunResult, JudgeTask, JudgeVerdict, WorkerHeartbeat,
};
use project_balloon_judge_worker::{
    heartbeat::{WorkerActivity, WorkerHeartbeatPublisher, WorkerHeartbeatPublisherConfig},
    rabbit::{
        InFlightTasks, JudgeTaskHandler, RabbitJudgeWorker, RabbitJudgeWorkerConfig, TaskFailure,
    },
};
use project_balloon_test_support::valid_judge_task;
use time::OffsetDateTime;
use tokio::sync::{Notify, watch};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires the reviewed RabbitMQ Judge topology"]
async fn capacity_two_runs_in_parallel_and_shutdown_drains_in_flight_tasks() {
    let amqp_url = env::var("PROJECT_BALLOON_TEST_AMQP_URL")
        .expect("PROJECT_BALLOON_TEST_AMQP_URL must be set");
    let connection = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .expect("connect RabbitMQ");
    let channel = connection.create_channel().await.expect("create RabbitMQ channel");
    channel.confirm_select(ConfirmSelectOptions::default()).await.expect("enable confirms");
    assert!(
        channel
            .basic_get("judge.tasks".into(), BasicGetOptions::default())
            .await
            .expect("inspect task queue")
            .is_none(),
        "task queue must be empty before the concurrency test"
    );

    let gate = Arc::new(GatedHandler::default());
    let activity = WorkerActivity::new(2);
    let worker = RabbitJudgeWorker::new(
        RabbitJudgeWorkerConfig {
            uri: amqp_url,
            task_queue: "judge.tasks".to_owned(),
            worker_id: "concurrency-test-worker".to_owned(),
            prefetch: 2,
            request_timeout: Duration::from_secs(5),
            reconnect_delay: Duration::from_millis(100),
            max_task_cases: 64,
            in_flight: InFlightTasks::new(),
            health: None,
        },
        gate.clone(),
        activity.clone(),
    );
    let (shutdown, shutdown_rx) = watch::channel(false);
    let worker_task = tokio::spawn(worker.run(shutdown_rx));

    let mut task_ids = Vec::new();
    for submission_id in [91_001, 91_002] {
        let mut task = valid_judge_task();
        task.judgement_id = Uuid::new_v4();
        task.submission_id = submission_id;
        publish_task(&channel, &task).await;
        task_ids.push(task.judgement_id);
    }
    tokio::time::timeout(Duration::from_secs(5), gate.both_started.notified())
        .await
        .expect("two tasks must start concurrently");
    assert_eq!(gate.maximum.load(Ordering::SeqCst), 2);
    assert_eq!(activity.active_tasks(), 2);

    let _sent = shutdown.send(true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!worker_task.is_finished(), "shutdown must wait for in-flight Judge tasks");
    gate.release.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), worker_task)
        .await
        .expect("Worker must drain promptly")
        .expect("Worker task must join");
    assert_eq!(activity.active_tasks(), 0);

    let mut observed = Vec::new();
    while observed.len() < task_ids.len() {
        let message = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(message) = channel
                    .basic_get("judge.results".into(), BasicGetOptions::default())
                    .await
                    .expect("poll result queue")
                {
                    break message;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("confirmed result must arrive");
        let result: JudgeResult = serde_json::from_slice(&message.data).expect("valid result");
        observed.push(result.judgement_id);
        message.ack(BasicAckOptions::default()).await.expect("ack observed result");
    }
    task_ids.sort_unstable();
    observed.sort_unstable();
    assert_eq!(observed, task_ids);
}

#[tokio::test]
#[ignore = "restarts the RabbitMQ Docker container"]
async fn broker_restart_requeues_unacknowledged_in_flight_task() {
    let amqp_url = env::var("PROJECT_BALLOON_TEST_AMQP_URL")
        .expect("PROJECT_BALLOON_TEST_AMQP_URL must be set");
    let rabbit_container = env::var("PROJECT_BALLOON_TEST_RABBITMQ_CONTAINER")
        .unwrap_or_else(|_| "project-balloon-it-rabbitmq".to_owned());
    let connection = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .expect("connect RabbitMQ");
    let channel = connection.create_channel().await.expect("create RabbitMQ channel");
    channel.confirm_select(ConfirmSelectOptions::default()).await.expect("enable confirms");
    assert!(
        channel
            .basic_get("judge.tasks".into(), BasicGetOptions::default())
            .await
            .expect("inspect task queue")
            .is_none(),
        "task queue must be empty before the recovery test"
    );
    assert!(
        channel
            .basic_get("judge.results".into(), BasicGetOptions::default())
            .await
            .expect("inspect result queue")
            .is_none(),
        "result queue must be empty before the recovery test"
    );

    let handler = Arc::new(RecoveryHandler::default());
    let worker = RabbitJudgeWorker::new(
        RabbitJudgeWorkerConfig {
            uri: amqp_url.clone(),
            task_queue: "judge.tasks".to_owned(),
            worker_id: "broker-recovery-test-worker".to_owned(),
            prefetch: 1,
            request_timeout: Duration::from_secs(2),
            reconnect_delay: Duration::from_millis(100),
            max_task_cases: 64,
            in_flight: InFlightTasks::new(),
            health: None,
        },
        handler.clone(),
        WorkerActivity::new(1),
    );
    let (shutdown, shutdown_rx) = watch::channel(false);
    let worker_task = tokio::spawn(worker.run(shutdown_rx));
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.submission_id = 92_001;
    publish_task(&channel, &task).await;
    tokio::time::timeout(Duration::from_secs(5), handler.first_started.notified())
        .await
        .expect("Worker must start the first delivery");

    Docker::connect_with_unix_defaults()
        .expect("connect Docker")
        .restart_container(&rabbit_container, None)
        .await
        .expect("restart RabbitMQ container");
    handler.release_first.notify_waiters();

    let recovered_connection = connect_with_retry(&amqp_url).await;
    let recovered_channel = recovered_connection.create_channel().await.expect("create channel");
    let message = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(message) = recovered_channel
                .basic_get("judge.results".into(), BasicGetOptions::default())
                .await
                .expect("poll recovered result queue")
            {
                break message;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("redelivered task must eventually publish a result");
    let result: JudgeResult = serde_json::from_slice(&message.data).expect("valid result");
    assert_eq!(result.judgement_id, task.judgement_id);
    assert!(handler.calls.load(Ordering::SeqCst) >= 2, "task must be redelivered");
    message.ack(BasicAckOptions::default()).await.expect("ack recovered result");
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        recovered_channel
            .basic_get("judge.results".into(), BasicGetOptions::default())
            .await
            .expect("check duplicate results")
            .is_none(),
        "a single task must not leave duplicate confirmed results"
    );

    let _sent = shutdown.send(true);
    tokio::time::timeout(Duration::from_secs(5), worker_task)
        .await
        .expect("Worker must stop after recovery")
        .expect("Worker task must join");
}

#[tokio::test]
#[ignore = "requires the reviewed RabbitMQ Judge topology"]
async fn worker_heartbeat_publisher_streams_activity_and_stops_on_shutdown() {
    let amqp_url = env::var("PROJECT_BALLOON_TEST_AMQP_URL")
        .expect("PROJECT_BALLOON_TEST_AMQP_URL must be set");
    let connection = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .expect("connect RabbitMQ");
    let channel = connection.create_channel().await.expect("create RabbitMQ channel");
    // The publisher only passively declares the exchange, so the test owns the
    // active declaration and binds a private queue to observe heartbeats.
    channel
        .exchange_declare(
            JUDGE_HEARTBEATS_EXCHANGE.into(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions { durable: true, ..ExchangeDeclareOptions::default() },
            FieldTable::default(),
        )
        .await
        .expect("declare heartbeat exchange");
    let queue = channel
        .queue_declare(
            "".into(),
            QueueDeclareOptions { exclusive: true, ..QueueDeclareOptions::default() },
            FieldTable::default(),
        )
        .await
        .expect("declare observation queue")
        .name()
        .as_str()
        .to_owned();
    channel
        .queue_bind(
            queue.as_str().into(),
            JUDGE_HEARTBEATS_EXCHANGE.into(),
            JUDGE_HEARTBEAT_ROUTING_KEY.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("bind heartbeat queue");

    let activity = WorkerActivity::new(4);
    let guard = activity.begin_task();
    let publisher = WorkerHeartbeatPublisher::new(
        WorkerHeartbeatPublisherConfig {
            uri: amqp_url.clone(),
            worker_id: "heartbeat-publisher-test".to_owned(),
            interval: Duration::from_millis(50),
            request_timeout: Duration::from_secs(5),
            reconnect_delay: Duration::from_millis(100),
            runtime_versions: std::collections::BTreeMap::from([(
                "cpp".to_owned(),
                "12.2.0".to_owned(),
            )]),
            sandbox_runtime: Some("runsc".to_owned()),
        },
        activity.clone(),
    );
    let (shutdown, shutdown_rx) = watch::channel(false);
    let publisher_task = tokio::spawn(publisher.run(shutdown_rx));

    let first = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(message) = channel
                .basic_get(queue.as_str().into(), BasicGetOptions::default())
                .await
                .expect("poll heartbeat queue")
            {
                break message;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("heartbeat must arrive while a task is active");
    let heartbeat: WorkerHeartbeat =
        serde_json::from_slice(&first.data).expect("deserialize heartbeat");
    assert_eq!(heartbeat.worker_id, "heartbeat-publisher-test");
    assert_eq!(heartbeat.capacity, 4);
    assert_eq!(heartbeat.active_tasks, 1);
    assert_eq!(heartbeat.runtime_versions.get("cpp").map(String::as_str), Some("12.2.0"));
    assert_eq!(heartbeat.sandbox_runtime.as_deref(), Some("runsc"));
    first.ack(BasicAckOptions::default()).await.expect("ack observed heartbeat");

    drop(guard);
    let second = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(message) = channel
                .basic_get(queue.as_str().into(), BasicGetOptions::default())
                .await
                .expect("poll heartbeat queue")
            {
                break message;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("second heartbeat must arrive after the task guard drops");
    let heartbeat: WorkerHeartbeat =
        serde_json::from_slice(&second.data).expect("deserialize second heartbeat");
    assert_eq!(heartbeat.active_tasks, 0);
    second.ack(BasicAckOptions::default()).await.expect("ack second heartbeat");

    let _sent = shutdown.send(true);
    tokio::time::timeout(Duration::from_secs(5), publisher_task)
        .await
        .expect("heartbeat publisher must stop after shutdown")
        .expect("heartbeat publisher task must join");
}

#[derive(Default)]
struct GatedHandler {
    active: AtomicU16,
    maximum: AtomicU16,
    both_started: Notify,
    release: Notify,
}

#[async_trait]
impl JudgeTaskHandler for GatedHandler {
    async fn handle(
        &self,
        task: &JudgeTask,
        _retry_count: u32,
    ) -> Result<JudgeResult, TaskFailure> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        if active == 2 {
            self.both_started.notify_one();
        }
        self.release.notified().await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        let now = OffsetDateTime::now_utc();
        Ok(JudgeResult {
            schema_version: JUDGE_RESULT_SCHEMA_VERSION,
            message_id: task.judgement_id,
            judgement_id: task.judgement_id,
            submission_id: task.submission_id,
            worker_id: "concurrency-test-worker".to_owned(),
            verdict: JudgeVerdict::Accepted,
            total_time_ms: 1,
            peak_memory_kb: 0,
            compile_log: None,
            started_at: now,
            completed_at: now,
            runs: vec![JudgeRunResult {
                test_index: 1,
                verdict: JudgeVerdict::Accepted,
                time_ms: 1,
                memory_kb: 0,
                exit_code: Some(0),
                stderr_tail: None,
            }],
        })
    }
}

#[derive(Default)]
struct RecoveryHandler {
    calls: AtomicU16,
    first_started: Notify,
    release_first: Notify,
}

#[async_trait]
impl JudgeTaskHandler for RecoveryHandler {
    async fn handle(
        &self,
        task: &JudgeTask,
        _retry_count: u32,
    ) -> Result<JudgeResult, TaskFailure> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            self.first_started.notify_one();
            self.release_first.notified().await;
        }
        Ok(accepted_result(task, "broker-recovery-test-worker"))
    }
}

fn accepted_result(task: &JudgeTask, worker_id: &str) -> JudgeResult {
    let now = OffsetDateTime::now_utc();
    JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: task.judgement_id,
        judgement_id: task.judgement_id,
        submission_id: task.submission_id,
        worker_id: worker_id.to_owned(),
        verdict: JudgeVerdict::Accepted,
        total_time_ms: 1,
        peak_memory_kb: 0,
        compile_log: None,
        started_at: now,
        completed_at: now,
        runs: vec![JudgeRunResult {
            test_index: 1,
            verdict: JudgeVerdict::Accepted,
            time_ms: 1,
            memory_kb: 0,
            exit_code: Some(0),
            stderr_tail: None,
        }],
    }
}

async fn connect_with_retry(amqp_url: &str) -> Connection {
    // A TCP/AMQP handshake started while RabbitMQ is stopping can itself hang
    // until the outer deadline, preventing the retry loop from making another
    // attempt after the broker is healthy. Bound each handshake separately and
    // retain the same two-minute window used by the dependency bootstrap.
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            match tokio::time::timeout(
                Duration::from_secs(5),
                Connection::connect(amqp_url, ConnectionProperties::default()),
            )
            .await
            {
                Ok(Ok(connection)) => break connection,
                Ok(Err(_)) | Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
    })
    .await
    .expect("RabbitMQ must become ready after restart")
}

async fn publish_task(channel: &lapin::Channel, task: &JudgeTask) {
    let payload = serde_json::to_vec(task).expect("serialize task");
    let confirmation = channel
        .basic_publish(
            JUDGE_TASKS_EXCHANGE.into(),
            "task".into(),
            BasicPublishOptions { mandatory: true, ..BasicPublishOptions::default() },
            &payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(task.judgement_id.to_string().into()),
        )
        .await
        .expect("publish task")
        .await
        .expect("confirm task");
    assert!(confirmation.is_ack());
}
