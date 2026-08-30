use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use project_balloon_api::{
    config::AppConfig,
    features::announcements::AnnouncementScheduleRunner,
    features::contests::ContestLifecycleRunner,
    features::judge_dispatch::{
        RabbitDeadLetterConsumer, RabbitJudgeResultConsumer, RabbitJudgeTaskPublisher,
        RabbitWorkerHeartbeatConsumer, SubmissionOutboxDispatcher,
        SubmissionOutboxDispatcherConfig, SubmissionStuckReaper,
    },
    features::printing::{CommandLineCupsGateway, CupsDeliveryRunner, CupsGateway},
    features::realtime::{DispatcherConfig, OutboxDispatcher, RealtimePublisher},
    features::resolver::ResolverAutoRunner,
    features::scoreboard::ScoreboardCache,
    features::submissions::{BatchRejudgeRunner, ExportTaskRunner, ExportTaskRunnerConfig},
    object_storage::{ObjectStorageHandle, S3ObjectStorage, S3ObjectStorageConfig},
    object_storage_cleanup::{ObjectStorageCleanupConfig, ObjectStorageCleanupRunner},
    router,
    state::AppState,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let config = load_config()?;
    let database = connect_database(&config).await?;
    let object_storage = init_object_storage(&config).await?;
    let cups_gateway = init_cups_gateway(&config);
    let judge_publisher = init_judge_publisher(&config);

    let state = build_app_state(
        &database,
        &config,
        object_storage.clone(),
        cups_gateway.clone(),
        judge_publisher.clone(),
    )
    .await?;
    let listener = TcpListener::bind(config.bind_address)
        .await
        .context("failed to bind the API listening socket")?;

    let shutdown = watch::Sender::new(false);
    let runners = spawn_background_runners(
        &database,
        &config,
        &state,
        object_storage,
        cups_gateway,
        judge_publisher,
        shutdown.clone(),
    )
    .await?;

    info!(address = %config.bind_address, "API listening");
    let server_result = axum::serve(
        listener,
        router(state, config.trusted_proxy_cidrs.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;

    runners.shutdown(database, server_result).await
}

fn load_config() -> Result<AppConfig> {
    let config = AppConfig::from_env().context("invalid API configuration")?;
    if config.uses_development_csrf_secret {
        tracing::warn!(
            "using development CSRF secret; set PROJECT_BALLOON_CSRF_SECRET before deployment"
        );
    }
    if !config.secure_cookies {
        tracing::warn!(
            "session and CSRF cookies are not Secure; set PROJECT_BALLOON_SECURE_COOKIES=true for HTTPS deployments"
        );
    }
    Ok(config)
}

async fn connect_database(config: &AppConfig) -> Result<PgPool> {
    let database = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .acquire_timeout(config.database_acquire_timeout)
        .connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")?;

    if config.run_migrations {
        MIGRATOR.run(&database).await.context("failed to run PostgreSQL migrations")?;
        info!("PostgreSQL migrations are current");
    }
    Ok(database)
}

async fn init_object_storage(config: &AppConfig) -> Result<Option<ObjectStorageHandle>> {
    let object_storage = if config.object_storage_enabled {
        Some(ObjectStorageHandle::with_buckets(
            Arc::new(S3ObjectStorage::new(S3ObjectStorageConfig {
                endpoint: config.object_storage_endpoint.clone(),
                region: config.object_storage_region.clone(),
                access_key: config.object_storage_access_key.clone(),
                secret_key: config.object_storage_secret_key.clone(),
                force_path_style: config.object_storage_force_path_style,
                request_timeout: config.object_storage_request_timeout,
            })?),
            config.object_storage_problem_bucket.clone(),
            config.object_storage_source_bucket.clone(),
        ))
    } else {
        None
    };
    if let Some(storage) = &object_storage {
        storage.ensure_buckets().await.context("failed to ensure object-storage buckets")?;
        info!("object-storage buckets are ready");
    }
    Ok(object_storage)
}

fn init_cups_gateway(config: &AppConfig) -> Option<Arc<dyn CupsGateway>> {
    config.cups_enabled.then(|| {
        Arc::new(CommandLineCupsGateway::new(
            config.cups_printer.clone(),
            config.cups_command_timeout,
        )) as Arc<dyn CupsGateway>
    })
}

fn init_judge_publisher(config: &AppConfig) -> Option<Arc<RabbitJudgeTaskPublisher>> {
    config.rabbitmq_enabled.then(|| {
        RabbitJudgeTaskPublisher::new(config.rabbitmq_url.clone(), config.rabbitmq_request_timeout)
    })
}

async fn build_app_state(
    database: &PgPool,
    config: &AppConfig,
    object_storage: Option<ObjectStorageHandle>,
    cups_gateway: Option<Arc<dyn CupsGateway>>,
    judge_publisher: Option<Arc<RabbitJudgeTaskPublisher>>,
) -> Result<AppState> {
    let mut state = match object_storage {
        Some(object_storage) => AppState::with_object_storage(
            database.clone(),
            config.readiness_timeout,
            config.session_ttl,
            config.secure_cookies,
            &config.csrf_secret,
            config.realtime_channel_capacity,
            config.realtime_redis_enabled,
            object_storage,
        ),
        None => AppState::new(
            database.clone(),
            config.readiness_timeout,
            config.session_ttl,
            config.secure_cookies,
            &config.csrf_secret,
            config.realtime_channel_capacity,
            config.realtime_redis_enabled,
        ),
    };
    state = state.with_deployment_mode(config.deployment_mode);
    if config.deployment_mode.is_competition() {
        state.competition().validate_schedule_integrity().await.map_err(|error| {
            anyhow::anyhow!("competition schedule validation failed: {error:?}")
        })?;
    }
    if let Some(publisher) = &judge_publisher {
        state = state.with_judge_publisher(publisher.clone());
    }
    if let Some(gateway) = &cups_gateway {
        state = state.with_cups_gateway(gateway.clone());
    }
    if config.scoreboard_cache_enabled {
        match tokio::time::timeout(
            config.scoreboard_cache_timeout,
            ScoreboardCache::connect(
                &config.redis_url,
                config.scoreboard_cache_ttl,
                config.scoreboard_cache_timeout,
            ),
        )
        .await
        {
            Ok(Ok(cache)) => {
                state = state.with_scoreboard_cache(cache);
                info!(
                    ttl_seconds = config.scoreboard_cache_ttl.as_secs(),
                    "scoreboard Redis cache enabled"
                );
            }
            Ok(Err(error)) => {
                warn!(%error, "scoreboard Redis cache unavailable; continuing with PostgreSQL");
            }
            Err(_) => {
                warn!("scoreboard Redis cache connection timed out; continuing with PostgreSQL")
            }
        }
    }
    Ok(state)
}

/// Every background task spawned by the API plus the channel that stops them.
struct BackgroundRunners {
    shutdown: watch::Sender<bool>,
    dispatcher: Option<JoinHandle<()>>,
    redis_subscriber: Option<JoinHandle<()>>,
    judge_dispatcher: Option<JoinHandle<()>>,
    judge_stuck_reaper: Option<JoinHandle<()>>,
    judge_result_consumer: Option<JoinHandle<()>>,
    worker_heartbeat_consumer: Option<JoinHandle<()>>,
    judge_dead_letter_consumer: Option<JoinHandle<()>>,
    cups_delivery: Option<JoinHandle<()>>,
    object_cleanup: Option<JoinHandle<()>>,
    export: Option<JoinHandle<()>>,
    batch_rejudge: JoinHandle<()>,
    resolver_auto: JoinHandle<()>,
    contest_lifecycle: JoinHandle<()>,
    announcement_schedule: JoinHandle<()>,
}

impl BackgroundRunners {
    /// Signals every runner, drains them, closes the pool, and applies the
    /// startup/shutdown error precedence of the original single `main`.
    async fn shutdown(self, database: PgPool, server_result: std::io::Result<()>) -> Result<()> {
        let _sent = self.shutdown.send(true);
        let dispatcher_result = match self.dispatcher {
            Some(task) => Some(task.await.context("realtime outbox dispatcher task failed")),
            None => None,
        };
        let subscriber_result = match self.redis_subscriber {
            Some(task) => Some(task.await.context("Redis realtime subscriber task failed")),
            None => None,
        };
        let judge_dispatcher_result = match self.judge_dispatcher {
            Some(task) => Some(task.await.context("submission outbox dispatcher task failed")),
            None => None,
        };
        let judge_stuck_reaper_result = match self.judge_stuck_reaper {
            Some(task) => Some(task.await.context("stuck-judging reaper task failed")),
            None => None,
        };
        let judge_result_consumer_result = match self.judge_result_consumer {
            Some(task) => Some(task.await.context("Judge result consumer task failed")),
            None => None,
        };
        let worker_heartbeat_consumer_result = match self.worker_heartbeat_consumer {
            Some(task) => Some(task.await.context("Worker heartbeat consumer task failed")),
            None => None,
        };
        let judge_dead_letter_consumer_result = match self.judge_dead_letter_consumer {
            Some(task) => Some(task.await.context("Judge dead-letter consumer task failed")),
            None => None,
        };
        let cups_delivery_result = match self.cups_delivery {
            Some(task) => Some(task.await.context("CUPS delivery runner task failed")),
            None => None,
        };
        let object_cleanup_result = match self.object_cleanup {
            Some(task) => Some(task.await.context("object-storage cleanup runner task failed")),
            None => None,
        };
        let export_result = match self.export {
            Some(task) => Some(task.await.context("submission export runner task failed")),
            None => None,
        };
        self.batch_rejudge.await.context("batch rejudge runner task failed")?;
        self.resolver_auto.await.context("Resolver auto-play runner task failed")?;
        self.contest_lifecycle.await.context("contest lifecycle runner task failed")?;
        self.announcement_schedule.await.context("announcement schedule runner task failed")?;
        database.close().await;
        server_result.context("API server failed")?;
        if let Some(result) = dispatcher_result {
            result?;
        }
        if let Some(result) = subscriber_result {
            result?;
        }
        if let Some(result) = judge_dispatcher_result {
            result?;
        }
        if let Some(result) = judge_stuck_reaper_result {
            result?;
        }
        if let Some(result) = judge_result_consumer_result {
            result?;
        }
        if let Some(result) = worker_heartbeat_consumer_result {
            result?;
        }
        if let Some(result) = judge_dead_letter_consumer_result {
            result?;
        }
        if let Some(result) = cups_delivery_result {
            result?;
        }
        if let Some(result) = object_cleanup_result {
            result?;
        }
        if let Some(result) = export_result {
            result?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn spawn_background_runners(
    database: &PgPool,
    config: &AppConfig,
    state: &AppState,
    object_storage: Option<ObjectStorageHandle>,
    cups_gateway: Option<Arc<dyn CupsGateway>>,
    judge_publisher: Option<Arc<RabbitJudgeTaskPublisher>>,
    shutdown: watch::Sender<bool>,
) -> Result<BackgroundRunners> {
    let shutdown_rx = shutdown.subscribe();
    let (publisher, redis_subscriber) = if config.realtime_redis_enabled {
        let (publisher, subscriber) = RealtimePublisher::connect_redis(
            &config.redis_url,
            config.realtime_redis_channel.clone(),
            state.realtime().clone(),
            config.realtime_redis_reconnect_delay,
        )
        .await
        .context("failed to initialize Redis realtime fanout")?;
        (publisher, Some(subscriber))
    } else {
        (RealtimePublisher::local(state.realtime().clone()), None)
    };
    let redis_subscriber_task =
        redis_subscriber.map(|subscriber| tokio::spawn(subscriber.run(shutdown_rx.clone())));
    let dispatcher_task = config.realtime_dispatcher_enabled.then(|| {
        tokio::spawn(
            OutboxDispatcher::new(
                database.clone(),
                publisher,
                DispatcherConfig {
                    poll_interval: config.realtime_poll_interval,
                    lease: config.realtime_lease,
                    retry_base: config.realtime_retry_base,
                    batch_size: config.realtime_batch_size,
                    max_attempts: config.realtime_max_attempts,
                },
            )
            .run(shutdown_rx.clone()),
        )
    });
    let judge_dispatcher_task = judge_publisher.as_ref().map(|publisher| {
        tokio::spawn(
            SubmissionOutboxDispatcher::new(
                database.clone(),
                publisher.clone(),
                SubmissionOutboxDispatcherConfig {
                    poll_interval: config.judge_dispatch_poll_interval,
                    lease: config.judge_dispatch_lease,
                    retry_base: config.judge_dispatch_retry_base,
                    batch_size: config.judge_dispatch_batch_size,
                    max_attempts: config.judge_dispatch_max_attempts,
                },
            )
            .run(shutdown_rx.clone()),
        )
    });
    // The reaper only makes sense when judge dispatch runs: without a
    // publisher a requeued row could never be sent again.
    let judge_stuck_reaper_task = judge_publisher.as_ref().map(|_| {
        tokio::spawn(
            SubmissionStuckReaper::new(
                database.clone(),
                config.judge_stuck_requeue_interval,
                config.judge_dispatch_max_attempts,
            )
            .run(shutdown_rx.clone()),
        )
    });
    let batch_rejudge_task =
        tokio::spawn(BatchRejudgeRunner::new(database.clone()).run(shutdown_rx.clone()));
    let cleanup_storage = object_storage.clone();
    let delivery_storage = object_storage.clone();
    let export_task = object_storage.map(|storage| {
        tokio::spawn(
            ExportTaskRunner::new(
                database.clone(),
                storage,
                ExportTaskRunnerConfig {
                    poll_interval: Duration::from_secs(2),
                    lease: Duration::from_secs(600),
                    retry_base: Duration::from_secs(5),
                    output_ttl: Duration::from_secs(86_400),
                },
            )
            .run(shutdown_rx.clone()),
        )
    });
    let resolver_auto_task =
        tokio::spawn(ResolverAutoRunner::new(database.clone()).run(shutdown_rx.clone()));
    let contest_lifecycle_task =
        tokio::spawn(ContestLifecycleRunner::new(database.clone()).run(shutdown_rx.clone()));
    let announcement_schedule_task =
        tokio::spawn(AnnouncementScheduleRunner::new(database.clone()).run(shutdown_rx.clone()));
    let judge_result_consumer_task = config.rabbitmq_enabled.then(|| {
        tokio::spawn(
            RabbitJudgeResultConsumer::new(
                database.clone(),
                config.rabbitmq_url.clone(),
                config.rabbitmq_request_timeout,
                config.judge_result_reconnect_delay,
                config.judge_result_prefetch,
            )
            .run(shutdown_rx.clone()),
        )
    });
    let worker_heartbeat_consumer_task = config.rabbitmq_enabled.then(|| {
        tokio::spawn(
            RabbitWorkerHeartbeatConsumer::new(
                database.clone(),
                config.rabbitmq_url.clone(),
                config.rabbitmq_request_timeout,
                config.judge_result_reconnect_delay,
                config.judge_result_prefetch,
            )
            .run(shutdown_rx.clone()),
        )
    });
    let judge_dead_letter_consumer_task = config.rabbitmq_enabled.then(|| {
        tokio::spawn(
            RabbitDeadLetterConsumer::new(
                database.clone(),
                config.rabbitmq_url.clone(),
                config.rabbitmq_request_timeout,
                config.judge_result_reconnect_delay,
                config.judge_result_prefetch,
            )
            .run(shutdown_rx.clone()),
        )
    });
    let cups_delivery_task = match (cups_gateway, delivery_storage) {
        (Some(gateway), Some(storage)) => Some(tokio::spawn(
            CupsDeliveryRunner::new(database.clone(), storage, gateway).run(shutdown_rx.clone()),
        )),
        (Some(_), None) => {
            warn!("CUPS delivery enabled without object storage; delivery runner is disabled");
            None
        }
        (None, _) => None,
    };
    let object_cleanup_task = cleanup_storage.map(|storage| {
        tokio::spawn(
            ObjectStorageCleanupRunner::new(
                database.clone(),
                storage,
                ObjectStorageCleanupConfig {
                    poll_interval: config.object_cleanup_poll_interval,
                    lease: config.object_cleanup_lease,
                    retry_base: config.object_cleanup_retry_base,
                    batch_size: config.object_cleanup_batch_size,
                },
            )
            .run(shutdown_rx.clone()),
        )
    });

    Ok(BackgroundRunners {
        shutdown,
        dispatcher: dispatcher_task,
        redis_subscriber: redis_subscriber_task,
        judge_dispatcher: judge_dispatcher_task,
        judge_stuck_reaper: judge_stuck_reaper_task,
        judge_result_consumer: judge_result_consumer_task,
        worker_heartbeat_consumer: worker_heartbeat_consumer_task,
        judge_dead_letter_consumer: judge_dead_letter_consumer_task,
        cups_delivery: cups_delivery_task,
        object_cleanup: object_cleanup_task,
        export: export_task,
        batch_rejudge: batch_rejudge_task,
        resolver_auto: resolver_auto_task,
        contest_lifecycle: contest_lifecycle_task,
        announcement_schedule: announcement_schedule_task,
    })
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("project_balloon_api=info"));
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
                Err(error) => {
                    tracing::error!(%error, "failed to install SIGTERM signal handler");
                }
            }
        };
        tokio::select! {
            () = ctrl_c => {}
            () = terminate => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
    info!("API shutdown requested");
}
