use std::{env, net::SocketAddr, time::Duration};

use ipnet::IpNet;
use thiserror::Error;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "postgres://127.0.0.1:5432/xcpc";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 20;
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS: u64 = 5;
const DEFAULT_READINESS_TIMEOUT_MILLISECONDS: u64 = 1_000;
const DEFAULT_SESSION_TTL_SECONDS: u64 = 43_200;
const DEFAULT_CSRF_SECRET: &str = "development-only-csrf-secret-change-me";
const DEFAULT_REALTIME_CHANNEL_CAPACITY: usize = 1_024;
const DEFAULT_REALTIME_POLL_MILLISECONDS: u64 = 250;
const DEFAULT_REALTIME_LEASE_SECONDS: u64 = 30;
const DEFAULT_REALTIME_RETRY_BASE_MILLISECONDS: u64 = 1_000;
const DEFAULT_REALTIME_BATCH_SIZE: i64 = 100;
const DEFAULT_REALTIME_MAX_ATTEMPTS: i32 = 8;
const DEFAULT_REALTIME_REDIS_CHANNEL: &str = "xcpc:realtime:events";
const DEFAULT_REALTIME_REDIS_RECONNECT_MILLISECONDS: u64 = 1_000;
const DEFAULT_SCOREBOARD_CACHE_TTL_SECONDS: u64 = 30;
const DEFAULT_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS: u64 = 200;
const DEFAULT_OBJECT_STORAGE_ENDPOINT: &str = "http://127.0.0.1:9000";
const DEFAULT_OBJECT_STORAGE_REGION: &str = "us-east-1";
const DEFAULT_OBJECT_STORAGE_PROBLEM_BUCKET: &str = "xcpc-problems";
const DEFAULT_OBJECT_STORAGE_SOURCE_BUCKET: &str = "xcpc-sources";
const DEFAULT_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS: u64 = 5_000;
const DEFAULT_OBJECT_CLEANUP_POLL_MILLISECONDS: u64 = 5_000;
const DEFAULT_OBJECT_CLEANUP_LEASE_SECONDS: u64 = 30;
const DEFAULT_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS: u64 = 1_000;
const DEFAULT_OBJECT_CLEANUP_BATCH_SIZE: i64 = 50;
const DEFAULT_JUDGE_DISPATCH_POLL_MILLISECONDS: u64 = 500;
const DEFAULT_JUDGE_DISPATCH_LEASE_SECONDS: u64 = 30;
const DEFAULT_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS: u64 = 1_000;
const DEFAULT_JUDGE_DISPATCH_BATCH_SIZE: i64 = 50;
const DEFAULT_JUDGE_DISPATCH_MAX_ATTEMPTS: i32 = 8;
const DEFAULT_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS: u64 = 5_000;
const DEFAULT_JUDGE_RESULT_PREFETCH: u16 = 32;
const DEFAULT_JUDGE_RESULT_RECONNECT_MILLISECONDS: u64 = 1_000;
const DEFAULT_CUPS_PRINTER: &str = "xcpc";
const DEFAULT_CUPS_COMMAND_TIMEOUT_MILLISECONDS: u64 = 5_000;
const DEFAULT_TRUSTED_PROXY_CIDRS: &str = "127.0.0.1/32,::1/128";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeploymentMode {
    #[default]
    Standard,
    Competition,
}

impl DeploymentMode {
    #[must_use]
    pub const fn is_competition(self) -> bool {
        matches!(self, Self::Competition)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Competition => "competition",
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub deployment_mode: DeploymentMode,
    pub bind_address: SocketAddr,
    pub trusted_proxy_cidrs: Vec<IpNet>,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_acquire_timeout: Duration,
    pub readiness_timeout: Duration,
    pub run_migrations: bool,
    pub session_ttl: Duration,
    pub secure_cookies: bool,
    pub csrf_secret: Vec<u8>,
    pub uses_development_csrf_secret: bool,
    pub allow_development_csrf_secret: bool,
    pub realtime_dispatcher_enabled: bool,
    pub realtime_channel_capacity: usize,
    pub realtime_poll_interval: Duration,
    pub realtime_lease: Duration,
    pub realtime_retry_base: Duration,
    pub realtime_batch_size: i64,
    pub realtime_max_attempts: i32,
    pub realtime_redis_enabled: bool,
    pub redis_url: String,
    pub realtime_redis_channel: String,
    pub realtime_redis_reconnect_delay: Duration,
    pub scoreboard_cache_enabled: bool,
    pub scoreboard_cache_ttl: Duration,
    pub scoreboard_cache_timeout: Duration,
    pub object_storage_enabled: bool,
    pub object_storage_endpoint: String,
    pub object_storage_region: String,
    pub object_storage_access_key: String,
    pub object_storage_secret_key: String,
    pub object_storage_problem_bucket: String,
    pub object_storage_source_bucket: String,
    pub object_storage_force_path_style: bool,
    pub object_storage_request_timeout: Duration,
    pub object_cleanup_poll_interval: Duration,
    pub object_cleanup_lease: Duration,
    pub object_cleanup_retry_base: Duration,
    pub object_cleanup_batch_size: i64,
    pub rabbitmq_enabled: bool,
    pub rabbitmq_url: String,
    pub rabbitmq_request_timeout: Duration,
    pub judge_dispatch_poll_interval: Duration,
    pub judge_dispatch_lease: Duration,
    pub judge_dispatch_retry_base: Duration,
    pub judge_dispatch_batch_size: i64,
    pub judge_dispatch_max_attempts: i32,
    pub judge_result_prefetch: u16,
    pub judge_result_reconnect_delay: Duration,
    pub cups_enabled: bool,
    pub cups_printer: String,
    pub cups_command_timeout: Duration,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{name} has invalid value {value:?}: {reason}")]
    Invalid { name: &'static str, value: String, reason: &'static str },
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, ConfigError> {
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
        let database_url =
            lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());
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
        let realtime_dispatcher_enabled = parse_bool(
            "PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED",
            lookup("PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED")
                .unwrap_or_else(|| "true".to_owned()),
        )?;
        let realtime_channel_capacity = parse_positive(
            "PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY",
            lookup("PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY")
                .unwrap_or_else(|| DEFAULT_REALTIME_CHANNEL_CAPACITY.to_string()),
        )?;
        let realtime_poll_milliseconds = parse_positive(
            "PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS",
            lookup("PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_REALTIME_POLL_MILLISECONDS.to_string()),
        )?;
        let realtime_lease_seconds = parse_positive(
            "PROJECT_BALLOON_REALTIME_LEASE_SECONDS",
            lookup("PROJECT_BALLOON_REALTIME_LEASE_SECONDS")
                .unwrap_or_else(|| DEFAULT_REALTIME_LEASE_SECONDS.to_string()),
        )?;
        let realtime_retry_base_milliseconds = parse_positive(
            "PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS",
            lookup("PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_REALTIME_RETRY_BASE_MILLISECONDS.to_string()),
        )?;
        let realtime_batch_size = parse_positive(
            "PROJECT_BALLOON_REALTIME_BATCH_SIZE",
            lookup("PROJECT_BALLOON_REALTIME_BATCH_SIZE")
                .unwrap_or_else(|| DEFAULT_REALTIME_BATCH_SIZE.to_string()),
        )?;
        let realtime_max_attempts = parse_positive(
            "PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS",
            lookup("PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS")
                .unwrap_or_else(|| DEFAULT_REALTIME_MAX_ATTEMPTS.to_string()),
        )?;
        let realtime_redis_enabled = parse_bool(
            "PROJECT_BALLOON_REALTIME_REDIS_ENABLED",
            lookup("PROJECT_BALLOON_REALTIME_REDIS_ENABLED").unwrap_or_else(|| "false".to_owned()),
        )?;
        let scoreboard_cache_enabled = parse_bool(
            "PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED",
            lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED")
                .unwrap_or_else(|| "false".to_owned()),
        )?;
        let scoreboard_cache_ttl_seconds = parse_positive(
            "PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS",
            lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS")
                .unwrap_or_else(|| DEFAULT_SCOREBOARD_CACHE_TTL_SECONDS.to_string()),
        )?;
        let scoreboard_cache_timeout_milliseconds = parse_positive(
            "PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS",
            lookup("PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS.to_string()),
        )?;
        let redis_url = lookup("REDIS_URL").unwrap_or_default();
        if (realtime_redis_enabled || scoreboard_cache_enabled) && redis_url.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name: "REDIS_URL",
                value: redis_url,
                reason: "must not be empty when Redis realtime fanout is enabled",
            });
        }
        let realtime_redis_channel = lookup("PROJECT_BALLOON_REALTIME_REDIS_CHANNEL")
            .unwrap_or_else(|| DEFAULT_REALTIME_REDIS_CHANNEL.to_owned());
        if realtime_redis_channel.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_REALTIME_REDIS_CHANNEL",
                value: realtime_redis_channel,
                reason: "must not be empty",
            });
        }
        let realtime_redis_reconnect_milliseconds = parse_positive(
            "PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS",
            lookup("PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_REALTIME_REDIS_RECONNECT_MILLISECONDS.to_string()),
        )?;
        let object_storage_enabled = parse_bool(
            "PROJECT_BALLOON_OBJECT_STORAGE_ENABLED",
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_ENABLED").unwrap_or_else(|| "false".to_owned()),
        )?;
        let object_storage_endpoint = lookup("PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT")
            .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_ENDPOINT.to_owned());
        let object_storage_region = lookup("PROJECT_BALLOON_OBJECT_STORAGE_REGION")
            .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_REGION.to_owned());
        let object_storage_access_key =
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY").unwrap_or_default();
        let object_storage_secret_key =
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY").unwrap_or_default();
        let object_storage_problem_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET")
            .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_PROBLEM_BUCKET.to_owned());
        let object_storage_source_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET")
            .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_SOURCE_BUCKET.to_owned());
        let object_storage_force_path_style = parse_bool(
            "PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE",
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE")
                .unwrap_or_else(|| "true".to_owned()),
        )?;
        let object_storage_request_timeout_milliseconds = parse_positive(
            "PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS",
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS.to_string()),
        )?;
        let object_cleanup_poll_milliseconds = parse_positive(
            "PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS",
            lookup("PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_POLL_MILLISECONDS.to_string()),
        )?;
        let object_cleanup_lease_seconds = parse_positive(
            "PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS",
            lookup("PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS")
                .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_LEASE_SECONDS.to_string()),
        )?;
        let object_cleanup_retry_base_milliseconds = parse_positive(
            "PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS",
            lookup("PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS.to_string()),
        )?;
        let object_cleanup_batch_size = parse_positive(
            "PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE",
            lookup("PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE")
                .unwrap_or_else(|| DEFAULT_OBJECT_CLEANUP_BATCH_SIZE.to_string()),
        )?;
        let cups_enabled = parse_bool(
            "PROJECT_BALLOON_CUPS_ENABLED",
            lookup("PROJECT_BALLOON_CUPS_ENABLED").unwrap_or_else(|| "false".to_owned()),
        )?;
        let cups_printer = lookup("PROJECT_BALLOON_CUPS_PRINTER")
            .unwrap_or_else(|| DEFAULT_CUPS_PRINTER.to_owned());
        if cups_enabled && cups_printer.trim().is_empty() {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_CUPS_PRINTER",
                value: cups_printer,
                reason: "must not be empty when CUPS is enabled",
            });
        }
        if cups_enabled && !object_storage_enabled {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_CUPS_ENABLED",
                value: "true".to_owned(),
                reason: "requires object storage to be enabled",
            });
        }
        let cups_command_timeout_milliseconds = parse_positive(
            "PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS",
            lookup("PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_CUPS_COMMAND_TIMEOUT_MILLISECONDS.to_string()),
        )?;
        let rabbitmq_enabled = parse_bool(
            "PROJECT_BALLOON_RABBITMQ_ENABLED",
            lookup("PROJECT_BALLOON_RABBITMQ_ENABLED").unwrap_or_else(|| "false".to_owned()),
        )?;
        let rabbitmq_url = lookup("PROJECT_BALLOON_RABBITMQ_URL").unwrap_or_default();
        if rabbitmq_enabled
            && (!rabbitmq_url.starts_with("amqp://") && !rabbitmq_url.starts_with("amqps://"))
        {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_RABBITMQ_URL",
                value: "[redacted]".to_owned(),
                reason: "must be an AMQP or AMQPS URL when RabbitMQ is enabled",
            });
        }
        let rabbitmq_request_timeout_milliseconds = parse_positive(
            "PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS",
            lookup("PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS.to_string()),
        )?;
        let judge_dispatch_poll_milliseconds = parse_positive(
            "PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS",
            lookup("PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_POLL_MILLISECONDS.to_string()),
        )?;
        let judge_dispatch_lease_seconds = parse_positive(
            "PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS",
            lookup("PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS")
                .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_LEASE_SECONDS.to_string()),
        )?;
        let judge_dispatch_retry_base_milliseconds = parse_positive(
            "PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS",
            lookup("PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS.to_string()),
        )?;
        let judge_dispatch_batch_size = parse_positive(
            "PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE",
            lookup("PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE")
                .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_BATCH_SIZE.to_string()),
        )?;
        let judge_dispatch_max_attempts = parse_positive(
            "PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS",
            lookup("PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS")
                .unwrap_or_else(|| DEFAULT_JUDGE_DISPATCH_MAX_ATTEMPTS.to_string()),
        )?;
        let judge_result_prefetch = parse_positive(
            "PROJECT_BALLOON_JUDGE_RESULT_PREFETCH",
            lookup("PROJECT_BALLOON_JUDGE_RESULT_PREFETCH")
                .unwrap_or_else(|| DEFAULT_JUDGE_RESULT_PREFETCH.to_string()),
        )?;
        let judge_result_reconnect_milliseconds = parse_positive(
            "PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS",
            lookup("PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS")
                .unwrap_or_else(|| DEFAULT_JUDGE_RESULT_RECONNECT_MILLISECONDS.to_string()),
        )?;
        if object_storage_enabled {
            for (name, value) in [
                ("PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT", &object_storage_endpoint),
                ("PROJECT_BALLOON_OBJECT_STORAGE_REGION", &object_storage_region),
                ("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY", &object_storage_access_key),
                ("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY", &object_storage_secret_key),
                ("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET", &object_storage_problem_bucket),
                ("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET", &object_storage_source_bucket),
            ] {
                if value.trim().is_empty() {
                    return Err(ConfigError::Invalid {
                        name,
                        value: if name.ends_with("KEY") {
                            "[redacted]".to_owned()
                        } else {
                            value.clone()
                        },
                        reason: "must not be empty when object storage is enabled",
                    });
                }
            }
            if !object_storage_endpoint.starts_with("http://")
                && !object_storage_endpoint.starts_with("https://")
            {
                return Err(ConfigError::Invalid {
                    name: "PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT",
                    value: object_storage_endpoint,
                    reason: "must be an HTTP or HTTPS URL",
                });
            }
        }

        Ok(Self {
            deployment_mode,
            bind_address,
            trusted_proxy_cidrs,
            database_url,
            database_max_connections,
            database_acquire_timeout: Duration::from_secs(acquire_timeout_seconds),
            readiness_timeout: Duration::from_millis(readiness_timeout_milliseconds),
            run_migrations,
            session_ttl: Duration::from_secs(session_ttl_seconds),
            secure_cookies,
            csrf_secret: csrf_secret.into_bytes(),
            uses_development_csrf_secret,
            allow_development_csrf_secret,
            realtime_dispatcher_enabled,
            realtime_channel_capacity,
            realtime_poll_interval: Duration::from_millis(realtime_poll_milliseconds),
            realtime_lease: Duration::from_secs(realtime_lease_seconds),
            realtime_retry_base: Duration::from_millis(realtime_retry_base_milliseconds),
            realtime_batch_size,
            realtime_max_attempts,
            realtime_redis_enabled,
            redis_url,
            realtime_redis_channel,
            realtime_redis_reconnect_delay: Duration::from_millis(
                realtime_redis_reconnect_milliseconds,
            ),
            scoreboard_cache_enabled,
            scoreboard_cache_ttl: Duration::from_secs(scoreboard_cache_ttl_seconds),
            scoreboard_cache_timeout: Duration::from_millis(scoreboard_cache_timeout_milliseconds),
            object_storage_enabled,
            object_storage_endpoint,
            object_storage_region,
            object_storage_access_key,
            object_storage_secret_key,
            object_storage_problem_bucket,
            object_storage_source_bucket,
            object_storage_force_path_style,
            object_storage_request_timeout: Duration::from_millis(
                object_storage_request_timeout_milliseconds,
            ),
            object_cleanup_poll_interval: Duration::from_millis(object_cleanup_poll_milliseconds),
            object_cleanup_lease: Duration::from_secs(object_cleanup_lease_seconds),
            object_cleanup_retry_base: Duration::from_millis(
                object_cleanup_retry_base_milliseconds,
            ),
            object_cleanup_batch_size,
            rabbitmq_enabled,
            rabbitmq_url,
            rabbitmq_request_timeout: Duration::from_millis(rabbitmq_request_timeout_milliseconds),
            judge_dispatch_poll_interval: Duration::from_millis(judge_dispatch_poll_milliseconds),
            judge_dispatch_lease: Duration::from_secs(judge_dispatch_lease_seconds),
            judge_dispatch_retry_base: Duration::from_millis(
                judge_dispatch_retry_base_milliseconds,
            ),
            judge_dispatch_batch_size,
            judge_dispatch_max_attempts,
            judge_result_prefetch,
            judge_result_reconnect_delay: Duration::from_millis(
                judge_result_reconnect_milliseconds,
            ),
            cups_enabled,
            cups_printer,
            cups_command_timeout: Duration::from_millis(cups_command_timeout_milliseconds),
        })
    }
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
    use std::{collections::HashMap, time::Duration};

    use super::{AppConfig, ConfigError, DeploymentMode};

    /// Wraps a value map so the development CSRF secret is explicitly allowed;
    /// every validation test below targets a different concern, not the CSRF
    /// default-secret rejection.
    fn dev_lookup<'a>(
        values: &'a HashMap<&'a str, String>,
    ) -> impl FnMut(&str) -> Option<String> + 'a {
        move |name| {
            if name == "PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET" {
                Some("true".to_owned())
            } else {
                values.get(name).cloned()
            }
        }
    }

    #[test]
    fn local_defaults_are_valid() {
        let config =
            AppConfig::from_lookup(dev_lookup(&HashMap::new())).expect("defaults must be valid");

        assert_eq!(config.bind_address.to_string(), "127.0.0.1:8080");
        assert_eq!(config.deployment_mode, DeploymentMode::Standard);
        assert_eq!(config.database_max_connections, 20);
        assert!(config.run_migrations);
        assert_eq!(config.session_ttl.as_secs(), 43_200);
        assert!(config.uses_development_csrf_secret);
        assert!(config.allow_development_csrf_secret);
        assert!(config.realtime_dispatcher_enabled);
        assert_eq!(config.realtime_batch_size, 100);
        assert!(!config.realtime_redis_enabled);
        assert_eq!(config.realtime_redis_channel, "xcpc:realtime:events");
        assert!(!config.scoreboard_cache_enabled);
        assert_eq!(config.scoreboard_cache_ttl.as_secs(), 30);
        assert_eq!(config.scoreboard_cache_timeout.as_millis(), 200);
        assert!(!config.object_storage_enabled);
        assert_eq!(config.object_cleanup_poll_interval, Duration::from_secs(5));
        assert_eq!(config.object_cleanup_batch_size, 50);
        assert!(!config.rabbitmq_enabled);
        assert_eq!(config.judge_dispatch_batch_size, 50);
        assert_eq!(config.judge_result_prefetch, 32);
        assert!(!config.cups_enabled);
    }

    #[test]
    fn deployment_mode_is_closed_and_accepts_competition() {
        let values = HashMap::from([("PROJECT_BALLOON_DEPLOYMENT_MODE", "competition".to_owned())]);
        let config = AppConfig::from_lookup(dev_lookup(&values)).expect("competition mode");
        assert_eq!(config.deployment_mode, DeploymentMode::Competition);
        assert_eq!(config.deployment_mode.as_str(), "competition");

        let invalid = HashMap::from([("PROJECT_BALLOON_DEPLOYMENT_MODE", "event".to_owned())]);
        assert!(matches!(
            AppConfig::from_lookup(dev_lookup(&invalid)),
            Err(ConfigError::Invalid { name: "PROJECT_BALLOON_DEPLOYMENT_MODE", .. })
        ));
    }

    #[test]
    fn development_csrf_secret_requires_explicit_opt_in() {
        let error = AppConfig::from_lookup(|_| None)
            .expect_err("default secret without opt-in must be rejected");
        assert_eq!(
            error,
            ConfigError::Invalid {
                name: "PROJECT_BALLOON_CSRF_SECRET",
                value: "[redacted]".to_owned(),
                reason: "must be explicitly set; the development secret is only permitted with PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET",
            }
        );
    }

    #[test]
    fn development_csrf_secret_with_secure_cookies_is_rejected_even_when_allowed() {
        let values = HashMap::from([
            ("PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET", "true".to_owned()),
            ("PROJECT_BALLOON_SECURE_COOKIES", "true".to_owned()),
        ]);
        let error = AppConfig::from_lookup(|name| values.get(name).cloned())
            .expect_err("secure cookies with the known secret must be rejected");
        assert_eq!(
            error,
            ConfigError::Invalid {
                name: "PROJECT_BALLOON_CSRF_SECRET",
                value: "[redacted]".to_owned(),
                reason: "must be explicitly changed when secure cookies are enabled",
            }
        );
    }

    #[test]
    fn zero_database_connections_are_rejected() {
        let values = HashMap::from([("PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS", "0".to_owned())]);
        let result = AppConfig::from_lookup(dev_lookup(&values));
        let error = match result {
            Ok(_) => panic!("zero pool size must fail"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ConfigError::Invalid {
                name: "PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS",
                value: "0".to_owned(),
                reason: "must be greater than zero",
            }
        );
    }

    #[test]
    fn migration_flag_has_closed_values() {
        let values = HashMap::from([("PROJECT_BALLOON_RUN_MIGRATIONS", "sometimes".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
    }

    #[test]
    fn enabled_cups_requires_a_printer_and_positive_timeout() {
        let missing_printer = HashMap::from([
            ("PROJECT_BALLOON_CUPS_ENABLED", "true".to_owned()),
            ("PROJECT_BALLOON_CUPS_PRINTER", String::new()),
            ("PROJECT_BALLOON_OBJECT_STORAGE_ENABLED", "true".to_owned()),
            ("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY", "key".to_owned()),
            ("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY", "secret".to_owned()),
        ]);
        assert!(AppConfig::from_lookup(dev_lookup(&missing_printer)).is_err());

        let zero_timeout =
            HashMap::from([("PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS", "0".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&zero_timeout)).is_err());

        let no_storage = HashMap::from([("PROJECT_BALLOON_CUPS_ENABLED", "true".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&no_storage)).is_err());
    }

    #[test]
    fn realtime_sizes_must_be_positive() {
        let values = HashMap::from([("PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY", "0".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
    }

    #[test]
    fn enabled_redis_requires_a_url() {
        let values = HashMap::from([
            ("PROJECT_BALLOON_REALTIME_REDIS_ENABLED", "true".to_owned()),
            ("REDIS_URL", String::new()),
        ]);
        assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
    }

    #[test]
    fn enabled_scoreboard_cache_requires_a_url_and_positive_ttl() {
        let missing_url = HashMap::from([
            ("PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED", "true".to_owned()),
            ("REDIS_URL", String::new()),
        ]);
        assert!(AppConfig::from_lookup(dev_lookup(&missing_url)).is_err());

        let zero_ttl =
            HashMap::from([("PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS", "0".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&zero_ttl)).is_err());
    }

    #[test]
    fn enabled_object_storage_requires_credentials() {
        let values = HashMap::from([("PROJECT_BALLOON_OBJECT_STORAGE_ENABLED", "true".to_owned())]);
        assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
    }

    #[test]
    fn enabled_rabbitmq_requires_an_amqp_url() {
        let values = HashMap::from([
            ("PROJECT_BALLOON_RABBITMQ_ENABLED", "true".to_owned()),
            ("PROJECT_BALLOON_RABBITMQ_URL", "http://rabbit.invalid".to_owned()),
        ]);
        assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
    }
}
