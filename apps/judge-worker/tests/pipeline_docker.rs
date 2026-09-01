use std::{
    env,
    io::{Cursor, Write},
    process::Command,
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
    JUDGE_TASK_SCHEMA_VERSION, JUDGE_TASKS_EXCHANGE, JudgeMode, JudgeResult, JudgeTask,
    JudgeVerdict,
};
use project_balloon_judge_worker::{
    artifacts::{ArtifactManager, S3ArtifactSource, S3ArtifactSourceConfig},
    heartbeat::WorkerActivity,
    rabbit::{InFlightTasks, RabbitJudgeWorker, RabbitJudgeWorkerConfig},
    sandbox::{DockerSandbox, DockerSandboxConfig},
    worker::JudgeEngine,
};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, watch};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// Both pipeline tests drive the shared broker queues; serialize them so
/// concurrent runtimes cannot consume each other's tasks or results.
static RABBIT_PIPELINE_GUARD: Mutex<()> = Mutex::const_new(());

#[tokio::test]
#[ignore = "requires RabbitMQ, RustFS, Docker, and fixed C/C++ runtime images"]
async fn rabbit_rustfs_cpp_pipeline_publishes_confirmed_result() {
    let _pipeline = RABBIT_PIPELINE_GUARD.lock().await;
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
        0,
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
        docker_connect_timeout_seconds: 10,
        docker_api_timeout: std::time::Duration::from_secs(5),
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
            max_task_cases: 64,
            in_flight: InFlightTasks::new(),
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

/// Interactive pipeline: the worker downloads the interactor artifact from the
/// problem bucket, and the harness shell exit codes drive the verdicts —
/// program crash maps to exit 10 (RuntimeError), interactor rejection to
/// exit 20 (WrongAnswer), and a clean exchange to Accepted.
#[tokio::test]
#[ignore = "requires RabbitMQ, RustFS, Docker, cc, and the fixed C runtime image"]
async fn rabbit_rustfs_interactive_pipeline_reports_interactor_verdicts() {
    let _pipeline = RABBIT_PIPELINE_GUARD.lock().await;
    let amqp_url = required_env("PROJECT_BALLOON_TEST_AMQP_URL");
    let endpoint = required_env("PROJECT_BALLOON_TEST_S3_ENDPOINT");
    let access_key = required_env("PROJECT_BALLOON_TEST_S3_ACCESS_KEY");
    let secret_key = required_env("PROJECT_BALLOON_TEST_S3_SECRET_KEY");
    let problem_bucket = env::var("PROJECT_BALLOON_TEST_S3_PROBLEM_BUCKET")
        .unwrap_or_else(|_| "xcpc-problems".to_owned());
    let source_bucket = env::var("PROJECT_BALLOON_TEST_S3_SOURCE_BUCKET")
        .unwrap_or_else(|_| "xcpc-sources".to_owned());

    let build_dir =
        env::temp_dir().join(format!("project-balloon-interactive-build-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&build_dir).await.expect("create build dir");
    let interactor_source = build_dir.join("interactor.c");
    let interactor_binary = build_dir.join("interactor");
    tokio::fs::write(
        &interactor_source,
        br#"#include <stdio.h>
#include <sys/select.h>
#include <sys/time.h>
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    FILE *f = fopen(argv[1], "r");
    if (!f) return 2;
    long long answer;
    if (fscanf(f, "%lld", &answer) != 1) return 2;
    fclose(f);
    printf("%lld\n", answer);
    fflush(stdout);
    /* The harness keeps the fifo write side open, so a vanished contestant
       never delivers EOF: bound the wait so the shell can exit 10. */
    fd_set replies;
    struct timeval bound = { 2, 0 };
    FD_ZERO(&replies);
    FD_SET(0, &replies);
    if (select(1, &replies, NULL, NULL, &bound) <= 0) return 1;
    long long reply;
    if (scanf("%lld", &reply) != 1) return 1;
    return reply == answer ? 0 : 1;
}
"#,
    )
    .await
    .expect("write interactor source");
    let status = Command::new("cc")
        .args(["-O2", "-static", "-o"])
        .arg(&interactor_binary)
        .arg(&interactor_source)
        .status()
        .expect("compile interactor");
    assert!(status.success(), "host cc must build the static interactor");
    let interactor = tokio::fs::read(&interactor_binary).await.expect("read interactor");
    let interactor_sha256 = hex::encode(Sha256::digest(&interactor));

    let testdata = Bytes::from(interactive_fixture_archive());
    let testdata_sha256 = hex::encode(Sha256::digest(&testdata));
    let testdata_key = "integration-worker/interactive-shared/testdata.zip".to_owned();
    let interactor_key = "integration-worker/interactive-shared/interactor".to_owned();
    put(&endpoint, &access_key, &secret_key, &problem_bucket, &testdata_key, testdata).await;
    put(
        &endpoint,
        &access_key,
        &secret_key,
        &problem_bucket,
        &interactor_key,
        Bytes::from(interactor),
    )
    .await;

    let cache =
        env::temp_dir().join(format!("project-balloon-interactive-pipeline-{}", Uuid::new_v4()));
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
        0,
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
        docker_connect_timeout_seconds: 10,
        docker_api_timeout: std::time::Duration::from_secs(5),
    })
    .expect("connect Docker sandbox");
    let engine =
        Arc::new(JudgeEngine::new("interactive-pipeline-worker".to_owned(), artifacts, sandbox));
    engine.preflight().await.expect("worker preflight");

    let connection = Connection::connect(&amqp_url, ConnectionProperties::default())
        .await
        .expect("connect RabbitMQ");
    let channel = connection.create_channel().await.expect("create RabbitMQ channel");
    channel.confirm_select(ConfirmSelectOptions::default()).await.expect("enable confirms");

    // Stale results from previously interrupted runs would masquerade as this
    // test's outcomes; drain the result queue before driving fresh tasks.
    while let Some(message) = channel
        .basic_get("judge.results".into(), BasicGetOptions::default())
        .await
        .expect("drain result queue")
    {
        message.ack(BasicAckOptions::default()).await.expect("ack drained result");
    }

    let (shutdown, shutdown_rx) = watch::channel(false);
    let worker = RabbitJudgeWorker::new(
        RabbitJudgeWorkerConfig {
            uri: amqp_url,
            task_queue: "judge.tasks".to_owned(),
            worker_id: "interactive-pipeline-worker".to_owned(),
            prefetch: 1,
            request_timeout: Duration::from_secs(5),
            reconnect_delay: Duration::from_millis(100),
            max_task_cases: 64,
            in_flight: InFlightTasks::new(),
            health: None,
        },
        engine,
        WorkerActivity::new(1),
    );
    let worker_task = tokio::spawn(worker.run(shutdown_rx));

    let scenarios: [(&str, &[u8], JudgeVerdict, i32); 3] = [
        (
            "echoes the interactor query",
            b"#include <stdio.h>\nint main(void){ long long x; if(scanf(\"%lld\",&x)!=1) return 1; printf(\"%lld\\n\",x); return 0; }\n",
            JudgeVerdict::Accepted,
            0,
        ),
        (
            "answers incorrectly",
            b"#include <stdio.h>\nint main(void){ long long x; if(scanf(\"%lld\",&x)!=1) return 1; printf(\"%lld\\n\",x+1); return 0; }\n",
            JudgeVerdict::WrongAnswer,
            20,
        ),
        (
            "crashes without replying",
            b"int main(void){ return 3; }\n",
            JudgeVerdict::RuntimeError,
            10,
        ),
    ];
    for (name, source, expected_verdict, expected_exit_code) in scenarios {
        let judgement_id = Uuid::new_v4();
        let source_sha256 = hex::encode(Sha256::digest(source));
        let source_key = format!("integration-worker/{judgement_id}/main.c");
        put(
            &endpoint,
            &access_key,
            &secret_key,
            &source_bucket,
            &source_key,
            Bytes::from_static(source),
        )
        .await;
        let task = JudgeTask {
            schema_version: JUDGE_TASK_SCHEMA_VERSION,
            judgement_id,
            submission_id: 9_100,
            problem_id: 9_100,
            testdata_version: 1,
            testdata_object_key: testdata_key.clone(),
            testdata_sha256: testdata_sha256.clone(),
            source_object_key: source_key.clone(),
            source_sha256,
            language: "c".to_owned(),
            time_limit_ms: 1_000,
            memory_limit_mb: 128,
            output_limit_kb: 64,
            language_multiplier: 1.0,
            judge_mode: JudgeMode::Interactive,
            interactor_object_key: Some(interactor_key.clone()),
            interactor_sha256: Some(interactor_sha256.clone()),
        };
        task.validate().expect("valid interactive task");
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
        let message = tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let Some(message) = channel
                    .basic_get("judge.results".into(), BasicGetOptions::default())
                    .await
                    .expect("poll result queue")
                else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let result: JudgeResult =
                    serde_json::from_slice(&message.data).expect("deserialize Worker result");
                if result.message_id == judgement_id {
                    break (message, result);
                }
                // A result for an earlier interrupted run: discard it and keep waiting.
                message.ack(BasicAckOptions::default()).await.expect("ack stale result");
            }
        })
        .await
        .expect("Worker must publish a result");
        let (message, result) = message;
        assert_eq!(
            result.verdict, expected_verdict,
            "scenario={name}, compile_log={:?}, runs={:?}",
            result.compile_log, result.runs
        );
        assert_eq!(result.runs.len(), 1, "scenario={name}");
        assert_eq!(
            result.runs[0].exit_code,
            Some(expected_exit_code),
            "scenario={name}, stderr={:?}",
            result.runs[0].stderr_tail
        );
        message.ack(BasicAckOptions::default()).await.expect("ack observed result");
        delete(&endpoint, &access_key, &secret_key, &source_bucket, &source_key).await;
    }

    let _sent = shutdown.send(true);
    worker_task.await.expect("Worker task joins");

    delete(&endpoint, &access_key, &secret_key, &problem_bucket, &testdata_key).await;
    delete(&endpoint, &access_key, &secret_key, &problem_bucket, &interactor_key).await;
    tokio::fs::remove_dir_all(cache).await.expect("remove pipeline cache");
    tokio::fs::remove_dir_all(build_dir).await.expect("remove build dir");
}

fn interactive_fixture_archive() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in [("1.in", b"42\n".as_slice()), ("1.out", b"42\n".as_slice())] {
        writer.start_file(name, options).expect("start fixture file");
        writer.write_all(content).expect("write fixture file");
    }
    writer.finish().expect("finish fixture archive").into_inner()
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
