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
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
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

    // One shutdown channel drives the graceful server stop, every background
    // runner, and the SSE streams (through AppState), so a single signal ends
    // all of them together.
    let shutdown = watch::Sender::new(false);
    let state = build_app_state(
        &database,
        &config,
        object_storage.clone(),
        cups_gateway.clone(),
        judge_publisher.clone(),
        shutdown.subscribe(),
    )
    .await?;
    let listener = TcpListener::bind(config.bind_address)
        .await
        .context("failed to bind the API listening socket")?;

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

/// Bound for establishing the startup PostgreSQL connection. sqlx bounds every
/// pool connection attempt by the acquire timeout, but an outer bound keeps a
/// stalled (as opposed to refused) database from hanging startup entirely.
const DATABASE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Parses the database URL into explicit connect options for the pool.
///
/// sqlx 0.9 does not expose TCP keepalive tuning on `PgConnectOptions` (unlike
/// the MySQL driver), so the operating system defaults govern keepalives; the
/// pool's acquire timeout still bounds every connection establishment.
fn database_connect_options(database_url: &str) -> Result<PgConnectOptions, sqlx::Error> {
    database_url.parse::<PgConnectOptions>()
}

async fn connect_database(config: &AppConfig) -> Result<PgPool> {
    let options = database_connect_options(&config.database_url)
        .context("invalid PostgreSQL connection URL")?;
    let database = tokio::time::timeout(
        DATABASE_CONNECT_TIMEOUT,
        PgPoolOptions::new()
            .max_connections(config.database_max_connections)
            .acquire_timeout(config.database_acquire_timeout)
            .connect_with(options),
    )
    .await
    .context("timed out connecting to PostgreSQL")?
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
                upload_timeout: config.object_storage_upload_timeout,
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
    shutdown: watch::Receiver<bool>,
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
    state = state.with_shutdown(shutdown);
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

/// Overall bound for draining the background runners after the shutdown
/// signal. Runners still alive when it expires are logged and aborted so
/// shutdown proceeds (pool close included) instead of hanging until the
/// orchestrator gives up and SIGKILLs the process.
const RUNNER_SHUTDOWN_DEADLINE: Duration = Duration::from_secs(30);

impl BackgroundRunners {
    /// Signals every runner, drains them, closes the pool, and applies the
    /// startup/shutdown error precedence of the original single `main`.
    async fn shutdown(self, database: PgPool, server_result: std::io::Result<()>) -> Result<()> {
        self.shutdown_within(database, server_result, RUNNER_SHUTDOWN_DEADLINE).await
    }

    async fn shutdown_within(
        self,
        database: PgPool,
        server_result: std::io::Result<()>,
        deadline: Duration,
    ) -> Result<()> {
        let _sent = self.shutdown.send(true);
        // Named in the order the original sequential shutdown awaited them, so
        // the same failure surfaces first when several runners fail together.
        let mut tasks: Vec<(&'static str, JoinHandle<()>)> = Vec::new();
        if let Some(task) = self.dispatcher {
            tasks.push(("realtime outbox dispatcher", task));
        }
        if let Some(task) = self.redis_subscriber {
            tasks.push(("Redis realtime subscriber", task));
        }
        if let Some(task) = self.judge_dispatcher {
            tasks.push(("submission outbox dispatcher", task));
        }
        if let Some(task) = self.judge_stuck_reaper {
            tasks.push(("stuck-judging reaper", task));
        }
        if let Some(task) = self.judge_result_consumer {
            tasks.push(("Judge result consumer", task));
        }
        if let Some(task) = self.worker_heartbeat_consumer {
            tasks.push(("Worker heartbeat consumer", task));
        }
        if let Some(task) = self.judge_dead_letter_consumer {
            tasks.push(("Judge dead-letter consumer", task));
        }
        if let Some(task) = self.cups_delivery {
            tasks.push(("CUPS delivery runner", task));
        }
        if let Some(task) = self.object_cleanup {
            tasks.push(("object-storage cleanup runner", task));
        }
        if let Some(task) = self.export {
            tasks.push(("submission export runner", task));
        }
        let optional_count = tasks.len();
        tasks.push(("batch rejudge runner", self.batch_rejudge));
        tasks.push(("Resolver auto-play runner", self.resolver_auto));
        tasks.push(("contest lifecycle runner", self.contest_lifecycle));
        tasks.push(("announcement schedule runner", self.announcement_schedule));

        let mut results = join_runners_within_deadline(tasks, deadline).await;
        let required_results = results.split_off(optional_count);

        database.close().await;
        for result in required_results {
            result?;
        }
        server_result.context("API server failed")?;
        for result in results {
            result?;
        }
        Ok(())
    }
}

/// Waits for every runner to finish inside an overall `deadline`; runners
/// still alive when it expires are logged and aborted so shutdown can proceed
/// (pool close included). Returns every runner's outcome in input order — a
/// runner aborted at the deadline reports its cancellation as a failure.
///
/// Each `JoinHandle` is awaited exactly once: the select below holds the
/// handle across the deadline instead of re-awaiting a completed handle.
async fn join_runners_within_deadline(
    tasks: Vec<(&'static str, JoinHandle<()>)>,
    deadline: Duration,
) -> Vec<anyhow::Result<()>> {
    futures_util::future::join_all(tasks.into_iter().map(|(name, mut task)| async move {
        let outcome = tokio::select! {
            result = &mut task => result,
            _ = tokio::time::sleep(deadline) => {
                warn!(runner = name, "background runner did not stop in time; aborting");
                task.abort();
                task.await
            }
        };
        outcome.context(format!("{name} task failed"))
    }))
    .await
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> PgPool {
        PgPoolOptions::new().connect_lazy_with(
            database_connect_options("postgres://127.0.0.1:5432/project-balloon-shutdown-test")
                .expect("database URL must parse"),
        )
    }

    #[test]
    fn database_connect_options_parse_the_database_url() {
        let options =
            database_connect_options("postgres://balloon:secret@db.internal:5433/contest")
                .expect("a valid database URL");
        assert_eq!(options.get_host(), "db.internal");
        assert_eq!(options.get_port(), 5433);
        assert_eq!(options.get_database(), Some("contest"));
        assert_eq!(options.get_username(), "balloon");
        assert!(database_connect_options("::not a url::").is_err());
    }

    #[tokio::test]
    async fn shutdown_joins_finished_runners_and_reports_clean_exit() {
        let runners = BackgroundRunners {
            shutdown: watch::Sender::new(false),
            dispatcher: Some(tokio::spawn(async {})),
            redis_subscriber: None,
            judge_dispatcher: Some(tokio::spawn(async {})),
            judge_stuck_reaper: None,
            judge_result_consumer: None,
            worker_heartbeat_consumer: None,
            judge_dead_letter_consumer: None,
            cups_delivery: None,
            object_cleanup: None,
            export: None,
            batch_rejudge: tokio::spawn(async {}),
            resolver_auto: tokio::spawn(async {}),
            contest_lifecycle: tokio::spawn(async {}),
            announcement_schedule: tokio::spawn(async {}),
        };
        runners
            .shutdown_within(test_pool(), Ok(()), Duration::from_secs(5))
            .await
            .expect("every runner stopped, so shutdown must succeed");
    }

    #[tokio::test]
    async fn shutdown_aborts_runners_that_outlive_the_deadline() {
        let runners = BackgroundRunners {
            shutdown: watch::Sender::new(false),
            dispatcher: None,
            redis_subscriber: None,
            judge_dispatcher: None,
            judge_stuck_reaper: None,
            judge_result_consumer: None,
            worker_heartbeat_consumer: None,
            judge_dead_letter_consumer: None,
            cups_delivery: None,
            object_cleanup: None,
            export: None,
            batch_rejudge: tokio::spawn(async {}),
            resolver_auto: tokio::spawn(std::future::pending::<()>()),
            contest_lifecycle: tokio::spawn(std::future::pending::<()>()),
            announcement_schedule: tokio::spawn(async {}),
        };
        let error = runners
            .shutdown_within(test_pool(), Ok(()), Duration::from_millis(50))
            .await
            .expect_err("runners that ignore shutdown must fail the shutdown");
        assert!(
            error.to_string().contains("Resolver auto-play runner task failed"),
            "unexpected error: {error:#}"
        );
    }
}
