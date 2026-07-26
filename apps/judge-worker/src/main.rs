use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use project_balloon_judge_worker::{
    WorkerConfig,
    artifacts::{ArtifactManager, S3ArtifactSource, S3ArtifactSourceConfig},
    heartbeat::{WorkerActivity, WorkerHeartbeatPublisher, WorkerHeartbeatPublisherConfig},
    rabbit::{RabbitJudgeWorker, RabbitJudgeWorkerConfig},
    sandbox::{DockerSandbox, DockerSandboxConfig},
    worker::JudgeEngine,
};
use tokio::sync::watch;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = WorkerConfig::from_env().context("invalid judge worker configuration")?;

    let artifact_source = Arc::new(S3ArtifactSource::new(S3ArtifactSourceConfig {
        endpoint: config.object_storage_endpoint.clone(),
        region: config.object_storage_region.clone(),
        access_key: config.object_storage_access_key.clone(),
        secret_key: config.object_storage_secret_key.clone(),
        request_timeout: config.request_timeout,
    }));
    let artifacts = ArtifactManager::new(
        artifact_source,
        config.cache_dir.clone(),
        config.problem_bucket.clone(),
        config.source_bucket.clone(),
        config.max_artifact_bytes,
    );
    let sandbox = DockerSandbox::connect(DockerSandboxConfig {
        socket: config.sandbox_socket.clone(),
        cache_dir: config.cache_dir.clone(),
        runtime: config.sandbox_runtime.clone(),
        user: config.sandbox_user.clone(),
        c_image: config.c_image.clone(),
        cpp_image: config.cpp_image.clone(),
        java_image: config.java_image.clone(),
        python_image: config.python_image.clone(),
    })?;
    let engine = Arc::new(JudgeEngine::new(config.worker_id.clone(), artifacts, sandbox));
    engine
        .preflight()
        .await
        .map_err(|error| anyhow::anyhow!(error.reason))
        .context("Judge worker preflight failed")?;

    info!(
        worker_id = config.worker_id,
        cache_dir = %config.cache_dir.display(),
        task_queue = config.task_queue,
        "Judge worker preflight complete"
    );
    let (shutdown, shutdown_rx) = watch::channel(false);
    let activity = WorkerActivity::new(config.task_prefetch);
    let runtime_versions = BTreeMap::from([
        ("c".to_owned(), image_version(&config.c_image)),
        ("cpp".to_owned(), image_version(&config.cpp_image)),
        ("java".to_owned(), image_version(&config.java_image)),
        ("python".to_owned(), image_version(&config.python_image)),
    ]);
    let worker = RabbitJudgeWorker::new(
        RabbitJudgeWorkerConfig {
            uri: config.amqp_url.clone(),
            task_queue: config.task_queue,
            worker_id: config.worker_id.clone(),
            prefetch: config.task_prefetch,
            request_timeout: config.request_timeout,
            reconnect_delay: config.reconnect_delay,
        },
        engine,
        activity.clone(),
    );
    let heartbeat = WorkerHeartbeatPublisher::new(
        WorkerHeartbeatPublisherConfig {
            uri: config.amqp_url,
            worker_id: config.worker_id,
            interval: config.heartbeat_interval,
            request_timeout: config.request_timeout,
            reconnect_delay: config.reconnect_delay,
            runtime_versions,
            sandbox_runtime: Some(
                config.sandbox_runtime.unwrap_or_else(|| "docker-default".to_owned()),
            ),
        },
        activity,
    );
    let worker_task = tokio::spawn(worker.run(shutdown_rx));
    let heartbeat_task = tokio::spawn(heartbeat.run(shutdown.subscribe()));
    shutdown_signal().await;
    info!("judge worker shutdown requested");
    let _sent = shutdown.send(true);
    worker_task.await.context("Judge worker task failed")?;
    heartbeat_task.await.context("Worker heartbeat task failed")?;
    Ok(())
}

fn image_version(image: &str) -> String {
    image.rsplit_once(':').map_or_else(|| image.to_owned(), |(_, tag)| tag.to_owned())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("project_balloon_judge_worker=info"));
    tracing_subscriber::fmt().with_env_filter(filter).json().init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl-C signal handler");
        }
    };
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = async {
            match signal(SignalKind::terminate()) {
                Ok(mut stream) => {
                    let _signal = stream.recv().await;
                }
                Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
            }
        };
        tokio::select! {
            () = ctrl_c => {}
            () = terminate => {}
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await;
}
