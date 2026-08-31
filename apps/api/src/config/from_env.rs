use std::{env, net::SocketAddr, time::Duration};

use ipnet::IpNet;

use super::*;

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    pub(super) fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let deployment = parse_deployment(&mut lookup)?;
        let database = parse_database(&mut lookup)?;
        let session = parse_session(&mut lookup)?;
        let realtime = parse_realtime(&mut lookup)?;
        let scoreboard_cache = parse_scoreboard_cache(&mut lookup)?;

        let redis_url = lookup("REDIS_URL").unwrap_or_default();
        if (realtime.redis_enabled || scoreboard_cache.enabled) && redis_url.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name: "REDIS_URL",
                value: redis_url,
                reason: "must not be empty when Redis realtime fanout is enabled",
            });
        }

        let realtime_redis = parse_realtime_redis(&mut lookup)?;
        let object_storage = parse_object_storage(&mut lookup)?;
        let cups = parse_cups(&mut lookup, object_storage.enabled)?;
        let rabbitmq = parse_rabbitmq(&mut lookup)?;
        let judge_dispatch = parse_judge_dispatch(&mut lookup)?;
        validate_object_storage_fields(&object_storage)?;

        Ok(Self {
            deployment_mode: deployment.deployment_mode,
            bind_address: deployment.bind_address,
            trusted_proxy_cidrs: deployment.trusted_proxy_cidrs,
            database_url: database.database_url,
            database_max_connections: database.database_max_connections,
            database_acquire_timeout: database.database_acquire_timeout,
            readiness_timeout: database.readiness_timeout,
            run_migrations: database.run_migrations,
            session_ttl: session.session_ttl,
            secure_cookies: session.secure_cookies,
            csrf_secret: session.csrf_secret,
            uses_development_csrf_secret: session.uses_development_csrf_secret,
            allow_development_csrf_secret: session.allow_development_csrf_secret,
            realtime_dispatcher_enabled: realtime.dispatcher_enabled,
            realtime_channel_capacity: realtime.channel_capacity,
            realtime_poll_interval: realtime.poll_interval,
            realtime_lease: realtime.lease,
            realtime_retry_base: realtime.retry_base,
            realtime_batch_size: realtime.batch_size,
            realtime_max_attempts: realtime.max_attempts,
            realtime_redis_enabled: realtime.redis_enabled,
            redis_url,
            realtime_redis_channel: realtime_redis.channel,
            realtime_redis_reconnect_delay: realtime_redis.reconnect_delay,
            scoreboard_cache_enabled: scoreboard_cache.enabled,
            scoreboard_cache_ttl: scoreboard_cache.ttl,
            scoreboard_cache_timeout: scoreboard_cache.timeout,
            object_storage_enabled: object_storage.enabled,
            object_storage_endpoint: object_storage.endpoint,
            object_storage_region: object_storage.region,
            object_storage_access_key: object_storage.access_key,
            object_storage_secret_key: object_storage.secret_key,
            object_storage_problem_bucket: object_storage.problem_bucket,
            object_storage_source_bucket: object_storage.source_bucket,
            object_storage_force_path_style: object_storage.force_path_style,
            object_storage_request_timeout: object_storage.request_timeout,
            object_cleanup_poll_interval: object_storage.cleanup_poll_interval,
            object_cleanup_lease: object_storage.cleanup_lease,
            object_cleanup_retry_base: object_storage.cleanup_retry_base,
            object_cleanup_batch_size: object_storage.cleanup_batch_size,
            rabbitmq_enabled: rabbitmq.enabled,
            rabbitmq_url: rabbitmq.url,
            rabbitmq_request_timeout: rabbitmq.request_timeout,
            judge_dispatch_poll_interval: judge_dispatch.poll_interval,
            judge_dispatch_lease: judge_dispatch.lease,
            judge_dispatch_retry_base: judge_dispatch.retry_base,
            judge_dispatch_batch_size: judge_dispatch.batch_size,
            judge_dispatch_max_attempts: judge_dispatch.max_attempts,
            judge_stuck_requeue_interval: judge_dispatch.stuck_requeue_interval,
            judge_result_prefetch: judge_dispatch.result_prefetch,
            judge_result_reconnect_delay: judge_dispatch.result_reconnect_delay,
            cups_enabled: cups.enabled,
            cups_printer: cups.printer,
            cups_command_timeout: cups.command_timeout,
        })
    }
}

struct DeploymentSettings {
    deployment_mode: DeploymentMode,
    bind_address: SocketAddr,
    trusted_proxy_cidrs: Vec<IpNet>,
}

fn parse_deployment(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<DeploymentSettings, ConfigError> {
    let deployment_mode = match lookup("PROJECT_BALLOON_DEPLOYMENT_MODE")
        .unwrap_or_else(|| "standard".to_owned())
        .to_ascii_lowercase()
        .as_str()
    {
        "standard" => DeploymentMode::Standard,
        "competition" => DeploymentMode::Competition,
        value => {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_DEPLOYMENT_MODE",
                value: value.to_owned(),
                reason: "expected standard or competition",
            });
        }
    };
    let bind_address = parse(
        "PROJECT_BALLOON_API_BIND",
        lookup("PROJECT_BALLOON_API_BIND").unwrap_or_else(|| DEFAULT_BIND_ADDRESS.to_owned()),
        "expected a socket address such as 127.0.0.1:8080",
    )?;
    let trusted_proxy_cidrs = parse_proxy_cidrs(
        lookup("PROJECT_BALLOON_TRUSTED_PROXY_CIDRS")
            .unwrap_or_else(|| DEFAULT_TRUSTED_PROXY_CIDRS.to_owned()),
    )?;
    Ok(DeploymentSettings { deployment_mode, bind_address, trusted_proxy_cidrs })
}

struct DatabaseSettings {
    database_url: String,
    database_max_connections: u32,
    database_acquire_timeout: Duration,
    readiness_timeout: Duration,
    run_migrations: bool,
}

fn parse_database(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<DatabaseSettings, ConfigError> {
    let database_url = lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());
    if database_url.trim().is_empty() {
        return Err(ConfigError::Invalid {
            name: "DATABASE_URL",
            value: database_url,
            reason: "must not be empty",
        });
    }

    let database_max_connections: u32 = parse(
        "PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS",
        lookup("PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS")
            .unwrap_or_else(|| DEFAULT_DATABASE_MAX_CONNECTIONS.to_string()),
        "expected a positive integer",
    )?;
    if database_max_connections == 0 {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS",
            value: database_max_connections.to_string(),
            reason: "must be greater than zero",
        });
    }

    let acquire_timeout_seconds: u64 = parse(
        "PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS",
        lookup("PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS")
            .unwrap_or_else(|| DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS.to_string()),
        "expected a positive integer number of seconds",
    )?;
    let readiness_timeout_milliseconds: u64 = parse(
        "PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS",
        lookup("PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_READINESS_TIMEOUT_MILLISECONDS.to_string()),
        "expected a positive integer number of milliseconds",
    )?;
    if acquire_timeout_seconds == 0 || readiness_timeout_milliseconds == 0 {
        let (name, value) = if acquire_timeout_seconds == 0 {
            ("PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS", "0")
        } else {
            ("PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS", "0")
        };
        return Err(ConfigError::Invalid {
            name,
            value: value.to_owned(),
            reason: "must be greater than zero",
        });
    }

    let run_migrations = parse_bool(
        "PROJECT_BALLOON_RUN_MIGRATIONS",
        lookup("PROJECT_BALLOON_RUN_MIGRATIONS").unwrap_or_else(|| "true".to_owned()),
    )?;

    Ok(DatabaseSettings {
        database_url,
        database_max_connections,
        database_acquire_timeout: Duration::from_secs(acquire_timeout_seconds),
        readiness_timeout: Duration::from_millis(readiness_timeout_milliseconds),
        run_migrations,
    })
}

struct SessionSettings {
    session_ttl: Duration,
    secure_cookies: bool,
    csrf_secret: Vec<u8>,
    uses_development_csrf_secret: bool,
    allow_development_csrf_secret: bool,
}

fn parse_session(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<SessionSettings, ConfigError> {
    let session_ttl_seconds: u64 = parse(
        "PROJECT_BALLOON_SESSION_TTL_SECONDS",
        lookup("PROJECT_BALLOON_SESSION_TTL_SECONDS")
            .unwrap_or_else(|| DEFAULT_SESSION_TTL_SECONDS.to_string()),
        "expected a positive integer number of seconds",
    )?;
    if session_ttl_seconds == 0 {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_SESSION_TTL_SECONDS",
            value: "0".to_owned(),
            reason: "must be greater than zero",
        });
    }
    let secure_cookies = parse_bool(
        "PROJECT_BALLOON_SECURE_COOKIES",
        lookup("PROJECT_BALLOON_SECURE_COOKIES").unwrap_or_else(|| "false".to_owned()),
    )?;
    let csrf_secret =
        lookup("PROJECT_BALLOON_CSRF_SECRET").unwrap_or_else(|| DEFAULT_CSRF_SECRET.to_owned());
    if csrf_secret.len() < 32 {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_CSRF_SECRET",
            value: "[redacted]".to_owned(),
            reason: "must contain at least 32 bytes",
        });
    }
    let allow_development_csrf_secret = parse_bool(
        "PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET",
        lookup("PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET").unwrap_or_else(|| "false".to_owned()),
    )?;
    let uses_development_csrf_secret = csrf_secret == DEFAULT_CSRF_SECRET;
    // The built-in secret is public knowledge, so anyone could forge CSRF
    // tokens if a deployment keeps it. Only an explicit development
    // opt-in may use it, and never together with secure cookies.
    if uses_development_csrf_secret && !allow_development_csrf_secret {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_CSRF_SECRET",
            value: "[redacted]".to_owned(),
            reason: "must be explicitly set; the development secret is only permitted with PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET",
        });
    }
    if secure_cookies && uses_development_csrf_secret {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_CSRF_SECRET",
            value: "[redacted]".to_owned(),
            reason: "must be explicitly changed when secure cookies are enabled",
        });
    }

    Ok(SessionSettings {
        session_ttl: Duration::from_secs(session_ttl_seconds),
        secure_cookies,
        csrf_secret: csrf_secret.into_bytes(),
        uses_development_csrf_secret,
        allow_development_csrf_secret,
    })
}

struct RealtimeSettings {
    dispatcher_enabled: bool,
    channel_capacity: usize,
    poll_interval: Duration,
    lease: Duration,
    retry_base: Duration,
    batch_size: i64,
    max_attempts: i32,
    redis_enabled: bool,
}

fn parse_realtime(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<RealtimeSettings, ConfigError> {
    let dispatcher_enabled = parse_bool(
        "PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED",
        lookup("PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED").unwrap_or_else(|| "true".to_owned()),
    )?;
    let channel_capacity = parse_positive(
        "PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY",
        lookup("PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY")
            .unwrap_or_else(|| DEFAULT_REALTIME_CHANNEL_CAPACITY.to_string()),
    )?;
    let poll_milliseconds = parse_positive(
        "PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS",
        lookup("PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_REALTIME_POLL_MILLISECONDS.to_string()),
    )?;
    let lease_seconds = parse_positive(
        "PROJECT_BALLOON_REALTIME_LEASE_SECONDS",
        lookup("PROJECT_BALLOON_REALTIME_LEASE_SECONDS")
            .unwrap_or_else(|| DEFAULT_REALTIME_LEASE_SECONDS.to_string()),
    )?;
    let retry_base_milliseconds = parse_positive(
        "PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS",
        lookup("PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_REALTIME_RETRY_BASE_MILLISECONDS.to_string()),
    )?;
    let batch_size = parse_positive(
        "PROJECT_BALLOON_REALTIME_BATCH_SIZE",
        lookup("PROJECT_BALLOON_REALTIME_BATCH_SIZE")
            .unwrap_or_else(|| DEFAULT_REALTIME_BATCH_SIZE.to_string()),
    )?;
    let max_attempts = parse_positive(
        "PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS",
        lookup("PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS")
            .unwrap_or_else(|| DEFAULT_REALTIME_MAX_ATTEMPTS.to_string()),
    )?;
    let redis_enabled = parse_bool(
        "PROJECT_BALLOON_REALTIME_REDIS_ENABLED",
        lookup("PROJECT_BALLOON_REALTIME_REDIS_ENABLED").unwrap_or_else(|| "false".to_owned()),
    )?;

    Ok(RealtimeSettings {
        dispatcher_enabled,
        channel_capacity,
        poll_interval: Duration::from_millis(poll_milliseconds),
        lease: Duration::from_secs(lease_seconds),
        retry_base: Duration::from_millis(retry_base_milliseconds),
        batch_size,
        max_attempts,
        redis_enabled,
    })
}

struct ScoreboardCacheSettings {
    enabled: bool,
    ttl: Duration,
    timeout: Duration,
}

fn parse_scoreboard_cache(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<ScoreboardCacheSettings, ConfigError> {
    let enabled = parse_bool(
        "PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED",
        lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED").unwrap_or_else(|| "false".to_owned()),
    )?;
    let ttl_seconds = parse_positive(
        "PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS",
        lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS")
            .unwrap_or_else(|| DEFAULT_SCOREBOARD_CACHE_TTL_SECONDS.to_string()),
    )?;
    let timeout_milliseconds = parse_positive(
        "PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS",
        lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS.to_string()),
    )?;

    Ok(ScoreboardCacheSettings {
        enabled,
        ttl: Duration::from_secs(ttl_seconds),
        timeout: Duration::from_millis(timeout_milliseconds),
    })
}

struct RealtimeRedisSettings {
    channel: String,
    reconnect_delay: Duration,
}

fn parse_realtime_redis(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<RealtimeRedisSettings, ConfigError> {
    let channel = lookup("PROJECT_BALLOON_REALTIME_REDIS_CHANNEL")
        .unwrap_or_else(|| DEFAULT_REALTIME_REDIS_CHANNEL.to_owned());
    if channel.trim().is_empty() {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_REALTIME_REDIS_CHANNEL",
            value: channel,
            reason: "must not be empty",
        });
    }
    let reconnect_milliseconds = parse_positive(
        "PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS",
        lookup("PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_REALTIME_REDIS_RECONNECT_MILLISECONDS.to_string()),
    )?;

    Ok(RealtimeRedisSettings {
        channel,
        reconnect_delay: Duration::from_millis(reconnect_milliseconds),
    })
}

struct ObjectStorageSettings {
    enabled: bool,
    endpoint: String,
    region: String,
    access_key: String,
    secret_key: String,
    problem_bucket: String,
    source_bucket: String,
    force_path_style: bool,
    request_timeout: Duration,
    cleanup_poll_interval: Duration,
    cleanup_lease: Duration,
    cleanup_retry_base: Duration,
    cleanup_batch_size: i64,
}

fn parse_object_storage(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<ObjectStorageSettings, ConfigError> {
    let enabled = parse_bool(
        "PROJECT_BALLOON_OBJECT_STORAGE_ENABLED",
        lookup("PROJECT_BALLOON_OBJECT_STORAGE_ENABLED").unwrap_or_else(|| "false".to_owned()),
    )?;
    let endpoint = lookup("PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT")
        .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_ENDPOINT.to_owned());
    let region = lookup("PROJECT_BALLOON_OBJECT_STORAGE_REGION")
        .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_REGION.to_owned());
    let access_key = lookup("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY").unwrap_or_default();
    let secret_key = lookup("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY").unwrap_or_default();
    let problem_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET")
        .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_PROBLEM_BUCKET.to_owned());
    let source_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET")
        .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_SOURCE_BUCKET.to_owned());
    let force_path_style = parse_bool(
        "PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE",
        lookup("PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE")
            .unwrap_or_else(|| "true".to_owned()),
    )?;
    let request_timeout_milliseconds = parse_positive(
        "PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS",
        lookup("PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS.to_string()),
    )?;
    let cleanup_poll_milliseconds = parse_positive(
        "PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS",
        lookup("PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_POLL_MILLISECONDS.to_string()),
    )?;
    let cleanup_lease_seconds = parse_positive(
        "PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS",
        lookup("PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS")
            .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_LEASE_SECONDS.to_string()),
    )?;
    let cleanup_retry_base_milliseconds = parse_positive(
        "PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS",
        lookup("PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS.to_string()),
    )?;
    let cleanup_batch_size = parse_positive(
        "PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE",
        lookup("PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE")
            .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_BATCH_SIZE.to_string()),
    )?;

    Ok(ObjectStorageSettings {
        enabled,
        endpoint,
        region,
        access_key,
        secret_key,
        problem_bucket,
        source_bucket,
        force_path_style,
        request_timeout: Duration::from_millis(request_timeout_milliseconds),
        cleanup_poll_interval: Duration::from_millis(cleanup_poll_milliseconds),
        cleanup_lease: Duration::from_secs(cleanup_lease_seconds),
        cleanup_retry_base: Duration::from_millis(cleanup_retry_base_milliseconds),
        cleanup_batch_size,
    })
}

/// Field-level object storage validation only applies when the subsystem is
/// enabled; it runs after the other domains so the error precedence matches
/// the original single-pass parser.
fn validate_object_storage_fields(settings: &ObjectStorageSettings) -> Result<(), ConfigError> {
    if !settings.enabled {
        return Ok(());
    }
    for (name, value) in [
        ("PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT", &settings.endpoint),
        ("PROJECT_BALLOON_OBJECT_STORAGE_REGION", &settings.region),
        ("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY", &settings.access_key),
        ("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY", &settings.secret_key),
        ("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET", &settings.problem_bucket),
        ("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET", &settings.source_bucket),
    ] {
        if value.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name,
                value: if name.ends_with("KEY") { "[redacted]".to_owned() } else { value.clone() },
                reason: "must not be empty when object storage is enabled",
            });
        }
    }
    if !settings.endpoint.starts_with("http://") && !settings.endpoint.starts_with("https://") {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT",
            value: settings.endpoint.clone(),
            reason: "must be an HTTP or HTTPS URL",
        });
    }
    Ok(())
}

struct CupsSettings {
    enabled: bool,
    printer: String,
    command_timeout: Duration,
}

fn parse_cups(
    lookup: &mut impl FnMut(&str) -> Option<String>,
    object_storage_enabled: bool,
) -> Result<CupsSettings, ConfigError> {
    let enabled = parse_bool(
        "PROJECT_BALLOON_CUPS_ENABLED",
        lookup("PROJECT_BALLOON_CUPS_ENABLED").unwrap_or_else(|| "false".to_owned()),
    )?;
    let printer =
        lookup("PROJECT_BALLOON_CUPS_PRINTER").unwrap_or_else(|| DEFAULT_CUPS_PRINTER.to_owned());
    if enabled && printer.trim().is_empty() {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_CUPS_PRINTER",
            value: printer,
            reason: "must not be empty when CUPS is enabled",
        });
    }
    if enabled && !object_storage_enabled {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_CUPS_ENABLED",
            value: "true".to_owned(),
            reason: "requires object storage to be enabled",
        });
    }
    let command_timeout_milliseconds = parse_positive(
        "PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS",
        lookup("PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_CUPS_COMMAND_TIMEOUT_MILLISECONDS.to_string()),
    )?;

    Ok(CupsSettings {
        enabled,
        printer,
        command_timeout: Duration::from_millis(command_timeout_milliseconds),
    })
}

struct RabbitMqSettings {
    enabled: bool,
    url: String,
    request_timeout: Duration,
}

fn parse_rabbitmq(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<RabbitMqSettings, ConfigError> {
    let enabled = parse_bool(
        "PROJECT_BALLOON_RABBITMQ_ENABLED",
        lookup("PROJECT_BALLOON_RABBITMQ_ENABLED").unwrap_or_else(|| "false".to_owned()),
    )?;
    let url = lookup("PROJECT_BALLOON_RABBITMQ_URL").unwrap_or_default();
    if enabled && (!url.starts_with("amqp://") && !url.starts_with("amqps://")) {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_RABBITMQ_URL",
            value: "[redacted]".to_owned(),
            reason: "must be an AMQP or AMQPS URL when RabbitMQ is enabled",
        });
    }
    let request_timeout_milliseconds = parse_positive(
        "PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS",
        lookup("PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS.to_string()),
    )?;

    Ok(RabbitMqSettings {
        enabled,
        url,
        request_timeout: Duration::from_millis(request_timeout_milliseconds),
    })
}

struct JudgeDispatchSettings {
    poll_interval: Duration,
    lease: Duration,
    retry_base: Duration,
    batch_size: i64,
    max_attempts: i32,
    stuck_requeue_interval: Duration,
    result_prefetch: u16,
    result_reconnect_delay: Duration,
}

fn parse_judge_dispatch(
    lookup: &mut impl FnMut(&str) -> Option<String>,
) -> Result<JudgeDispatchSettings, ConfigError> {
    let poll_milliseconds = parse_positive(
        "PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS",
        lookup("PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_POLL_MILLISECONDS.to_string()),
    )?;
    let lease_seconds = parse_positive(
        "PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS",
        lookup("PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS")
            .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_LEASE_SECONDS.to_string()),
    )?;
    let retry_base_milliseconds = parse_positive(
        "PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS",
        lookup("PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS.to_string()),
    )?;
    let batch_size = parse_positive(
        "PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE",
        lookup("PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE")
            .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_BATCH_SIZE.to_string()),
    )?;
    let max_attempts = parse_positive(
        "PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS",
        lookup("PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS")
            .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_MAX_ATTEMPTS.to_string()),
    )?;
    let stuck_requeue_interval_seconds = parse_positive(
        "PROJECT_BALLOON_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS",
        lookup("PROJECT_BALLOON_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS")
            .unwrap_or_else(|| DEFAULT_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS.to_string()),
    )?;
    let result_prefetch = parse_positive(
        "PROJECT_BALLOON_JUDGE_RESULT_PREFETCH",
        lookup("PROJECT_BALLOON_JUDGE_RESULT_PREFETCH")
            .unwrap_or_else(|| DEFAULT_JUDGE_RESULT_PREFETCH.to_string()),
    )?;
    let result_reconnect_milliseconds = parse_positive(
        "PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS",
        lookup("PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS")
            .unwrap_or_else(|| DEFAULT_JUDGE_RESULT_RECONNECT_MILLISECONDS.to_string()),
    )?;

    Ok(JudgeDispatchSettings {
        poll_interval: Duration::from_millis(poll_milliseconds),
        lease: Duration::from_secs(lease_seconds),
        retry_base: Duration::from_millis(retry_base_milliseconds),
        batch_size,
        max_attempts,
        stuck_requeue_interval: Duration::from_secs(stuck_requeue_interval_seconds),
        result_prefetch,
        result_reconnect_delay: Duration::from_millis(result_reconnect_milliseconds),
    })
}

fn parse_proxy_cidrs(value: String) -> Result<Vec<IpNet>, ConfigError> {
    let parsed: Result<Vec<_>, _> =
        value.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::parse).collect();
    let parsed = parsed.map_err(|_| ConfigError::Invalid {
        name: "PROJECT_BALLOON_TRUSTED_PROXY_CIDRS",
        value: value.clone(),
        reason: "expected a comma-separated list of IP CIDRs",
    })?;
    if parsed.is_empty() {
        return Err(ConfigError::Invalid {
            name: "PROJECT_BALLOON_TRUSTED_PROXY_CIDRS",
            value,
            reason: "must contain at least one CIDR",
        });
    }
    Ok(parsed)
}

fn parse<T>(name: &'static str, value: String, reason: &'static str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| ConfigError::Invalid { name, value, reason })
}

fn parse_bool(name: &'static str, value: String) -> Result<bool, ConfigError> {
    match value.as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(ConfigError::Invalid { name, value, reason: "expected true, false, 1, or 0" }),
    }
}

fn parse_positive<T>(name: &'static str, value: String) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let parsed = value.parse().map_err(|_| ConfigError::Invalid {
        name,
        value: value.clone(),
        reason: "expected a positive integer",
    })?;
    if parsed <= T::from(0) {
        return Err(ConfigError::Invalid { name, value, reason: "must be greater than zero" });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::parse_proxy_cidrs;
    use crate::config::ConfigError;

    #[test]
    fn parses_comma_separated_cidrs_and_trims_whitespace() {
        let cidrs = parse_proxy_cidrs(" 10.0.0.0/8 ,192.168.0.0/16,,2001:db8::/32 ".to_owned())
            .expect("valid CIDR list");
        assert_eq!(cidrs.len(), 3);
    }

    #[test]
    fn rejects_malformed_cidr_values() {
        for value in ["not-a-cidr", "10.0.0.1", "300.0.0.0/8"] {
            assert_eq!(
                parse_proxy_cidrs(value.to_owned()),
                Err(ConfigError::Invalid {
                    name: "PROJECT_BALLOON_TRUSTED_PROXY_CIDRS",
                    value: value.to_owned(),
                    reason: "expected a comma-separated list of IP CIDRs",
                })
            );
        }
    }

    #[test]
    fn rejects_lists_without_any_cidr() {
        assert_eq!(
            parse_proxy_cidrs("   ".to_owned()),
            Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_TRUSTED_PROXY_CIDRS",
                value: "   ".to_owned(),
                reason: "must contain at least one CIDR",
            })
        );
    }
}
