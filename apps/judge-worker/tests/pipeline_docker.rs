use std::{
    env,
    io::{Cursor, Write},
    sync::Arc,
    time::Duration,
};

use bollard::Docker;
use bytes::Bytes;
use lapin::{
    BasicProperties, Connection, ConnectionProperties,
    options::{BasicAckOptions, BasicGetOptions, BasicPublishOptions, ConfirmSelectOptions},
};
use object_store::{ObjectStoreExt, aws::AmazonS3Builder, path::Path};
use project_balloon_contracts::{
    JUDGE_TASK_SCHEMA_VERSION, JUDGE_TASKS_EXCHANGE, JudgeResult, JudgeTask, JudgeVerdict,
};
use project_balloon_judge_worker::{
    artifacts::{ArtifactManager, S3ArtifactSource, S3ArtifactSourceConfig},
    heartbeat::WorkerActivity,
    rabbit::{RabbitJudgeWorker, RabbitJudgeWorkerConfig},
    sandbox::{DockerSandbox, DockerSandboxConfig},
    worker::JudgeEngine,
};
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[tokio::test]
#[ignore = "requires RabbitMQ, RustFS, Docker, and fixed C/C++ runtime images"]
async fn rabbit_rustfs_cpp_pipeline_publishes_confirmed_result() {
    let amqp_url = required_env("PROJECT_BALLOON_TEST_AMQP_URL");
    let endpoint = required_env("PROJECT_BALLOON_TEST_S3_ENDPOINT");
    let access_key = required_env("PROJECT_BALLOON_TEST_S3_ACCESS_KEY");
    let secret_key = required_env("PROJECT_BALLOON_TEST_S3_SECRET_KEY");
    let problem_bucket = env::var("PROJECT_BALLOON_TEST_S3_PROBLEM_BUCKET")
        .unwrap_or_else(|_| "xcpc-problems".to_owned());
    let source_bucket = env::var("PROJECT_BALLOON_TEST_S3_SOURCE_BUCKET")
        .unwrap_or_else(|_| "xcpc-sources".to_owned());
    let source = Bytes::from_static(
        b"#include <iostream>\nint main(){ long long a,b; std::cin>>a>>b; std::cout<<a+b<<'\\n'; }\n",
    );
    let testdata = Bytes::from(fixture_archive());
    let source_sha256 = hex::encode(Sha256::digest(&source));
    let testdata_sha256 = hex::encode(Sha256::digest(&testdata));
    let judgement_id = Uuid::new_v4();
    let source_key = format!("integration-worker/{judgement_id}/main.cpp");
    let testdata_key = format!("integration-worker/{judgement_id}/testdata.zip");
    put(&endpoint, &access_key, &secret_key, &source_bucket, &source_key, source.clone()).await;
    put(&endpoint, &access_key, &secret_key, &problem_bucket, &testdata_key, testdata.clone())
        .await;

    let task = JudgeTask {
        schema_version: JUDGE_TASK_SCHEMA_VERSION,
        judgement_id,
        submission_id: 9_001,
        problem_id: 9_001,
        testdata_version: 1,
        testdata_object_key: testdata_key.clone(),
        testdata_sha256,
        source_object_key: source_key.clone(),
        source_sha256,
        language: "cpp".to_owned(),
        time_limit_ms: 1_000,
        memory_limit_mb: 128,
        output_limit_kb: 64,
        language_multiplier: 1.0,
        judge_mode: project_balloon_contracts::JudgeMode::Standard,
        interactor_object_key: None,
        interactor_sha256: None,
    };
    task.validate().expect("valid integration task");
    let cache = std::env::temp_dir().join(format!("project-balloon-pipeline-{judgement_id}"));
    let artifacts = ArtifactManager::new(
        Arc::new(
            S3ArtifactSource::new(S3ArtifactSourceConfig {
                endpoint: endpoint.clone(),
                region: "us-east-1".to_owned(),
                access_key: access_key.clone(),
                secret_key: secret_key.clone(),
                request_timeout: Duration::from_secs(5),
            })
            .expect("artifact storage credentials must be valid"),
        ),
        cache.clone(),
        problem_bucket.clone(),
        source_bucket.clone(),
        10 * 1024 * 1024,
    );
    Docker::connect_with_unix_defaults().expect("Docker socket must be available");
    let sandbox = DockerSandbox::connect(DockerSandboxConfig {
        socket: "/var/run/docker.sock".into(),
        cache_dir: cache.clone(),
        runtime: None,
        user: env::var("PROJECT_BALLOON_TEST_SANDBOX_USER")
            .unwrap_or_else(|_| "1000:1000".to_owned()),
        c_image: "judge-runtime-c:12.2.0".to_owned(),
        cpp_image: "judge-runtime-cpp:12.2.0".to_owned(),
        java_image: "judge-runtime-java:21".to_owned(),
        python_image: "judge-runtime-python:3.12.13".to_owned(),
    })
    .expect("connect Docker sandbox");
    let engine = Arc::new(JudgeEngine::new("pipeline-worker".to_owned(), artifacts, sandbox));
    engine.preflight().await.expect("worker preflight");

    let connection = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .expect("connect RabbitMQ");
    let channel = connection.create_channel().await.expect("create RabbitMQ channel");
    channel.confirm_select(ConfirmSelectOptions::default()).await.expect("enable confirms");
    assert!(
        channel
            .basic_get("judge.results".into(), BasicGetOptions::default())
            .await
            .expect("check result queue")
            .is_none(),
        "result queue must be empty before this isolated test"
    );
    let payload = serde_json::to_vec(&task).expect("serialize task");
    let confirmation = channel
        .basic_publish(
            JUDGE_TASKS_EXCHANGE.into(),
            "task".into(),
            BasicPublishOptions { mandatory: true, ..BasicPublishOptions::default() },
            &payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(judgement_id.to_string().into()),
        )
        .await
        .expect("publish task")
        .await
        .expect("confirm task");
    assert!(confirmation.is_ack());

    let (shutdown, shutdown_rx) = watch::channel(false);
    let worker = RabbitJudgeWorker::new(
        RabbitJudgeWorkerConfig {
            uri: amqp_url,
            task_queue: "judge.tasks".to_owned(),
            worker_id: "pipeline-worker".to_owned(),
            prefetch: 1,
            request_timeout: Duration::from_secs(5),
            reconnect_delay: Duration::from_millis(100),
            health: None,
        },
        engine,
        WorkerActivity::new(1),
    );
    let worker_task = tokio::spawn(worker.run(shutdown_rx));
    let message = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(message) = channel
                .basic_get("judge.results".into(), BasicGetOptions::default())
                .await
                .expect("poll result queue")
            {
                break message;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Worker must publish a result");
    let result: JudgeResult =
        serde_json::from_slice(&message.data).expect("deserialize Worker result");
    assert_eq!(result.message_id, judgement_id);
    assert_eq!(
        result.verdict,
        JudgeVerdict::Accepted,
        "compile_log={:?}, runs={:?}",
        result.compile_log,
        result.runs
    );
    assert_eq!(result.runs.len(), 2);
    message.ack(BasicAckOptions::default()).await.expect("ack observed result");
    let _sent = shutdown.send(true);
    worker_task.await.expect("Worker task joins");

    delete(&endpoint, &access_key, &secret_key, &source_bucket, &source_key).await;
    delete(&endpoint, &access_key, &secret_key, &problem_bucket, &testdata_key).await;
    tokio::fs::remove_dir_all(cache).await.expect("remove pipeline cache");
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this integration test"))
}

fn s3_bucket(
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
) -> object_store::aws::AmazonS3 {
    AmazonS3Builder::new()
        .with_endpoint(endpoint)
        .with_region("us-east-1")
        .with_bucket_name(bucket)
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key)
        .with_allow_http(endpoint.starts_with("http://"))
        .with_virtual_hosted_style_request(false)
        .build()
        .expect("pipeline test bucket configuration must be valid")
}

async fn put(
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
    key: &str,
    content: Bytes,
) {
    s3_bucket(endpoint, access_key, secret_key, bucket)
        .put(&Path::parse(key).expect("pipeline object key must be valid"), content.into())
        .await
        .expect("upload integration artifact");
}

async fn delete(endpoint: &str, access_key: &str, secret_key: &str, bucket: &str, key: &str) {
    s3_bucket(endpoint, access_key, secret_key, bucket)
        .delete(&Path::parse(key).expect("pipeline object key must be valid"))
        .await
        .expect("delete integration artifact");
}

fn fixture_archive() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in [
        ("1.in", b"2 3\n".as_slice()),
        ("1.out", b"5\n".as_slice()),
        ("2.in", b"40 2\n".as_slice()),
        ("2.out", b"42\n".as_slice()),
    ] {
        writer.start_file(name, options).expect("start fixture file");
        writer.write_all(content).expect("write fixture file");
    }
    writer.finish().expect("finish fixture archive").into_inner()
}
