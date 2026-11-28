use std::{net::SocketAddr, time::Duration};

use ipnet::IpNet;
use thiserror::Error;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "postgres://127.0.0.1:5432/xcpc";
const DEFAULT_DATABASE_MAX_CONNECTIONS: u32 = 20;
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_SECONDS: u64 = 5;
const DEFAULT_READINESS_TIMEOUT_MILLISECONDS: u64 = 1_000;
const DEFAULT_SESSION_TTL_SECONDS: u64 = 43_200;
/// Sized for a contest venue where hundreds of teams share one NAT address;
/// a value around ten (the pre-audit behaviour) locked the whole room out on
/// ordinary typos.
const DEFAULT_LOGIN_ATTEMPT_LIMIT_PER_IP: i64 = 100;
const DEFAULT_LOGIN_ACCOUNT_ATTEMPT_LIMIT: i64 = 10;
/// Upper bound of either login window. A larger value is a misconfiguration
/// that removes brute-force protection rather than tuning it.
const MAX_LOGIN_ATTEMPT_LIMIT: i64 = 10_000;
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
const DEFAULT_REDIS_OPERATION_TIMEOUT_MILLISECONDS: u64 = 200;
const DEFAULT_REDIS_POOL_SIZE: usize = 8;
const DEFAULT_OBJECT_STORAGE_ENDPOINT: &str = "http://127.0.0.1:9000";
const DEFAULT_OBJECT_STORAGE_REGION: &str = "us-east-1";
const DEFAULT_OBJECT_STORAGE_PROBLEM_BUCKET: &str = "xcpc-problems";
const DEFAULT_OBJECT_STORAGE_SOURCE_BUCKET: &str = "xcpc-sources";
const DEFAULT_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS: u64 = 5_000;
const DEFAULT_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS: u64 = 300_000;
const DEFAULT_OBJECT_CLEANUP_POLL_MILLISECONDS: u64 = 5_000;
const DEFAULT_OBJECT_CLEANUP_LEASE_SECONDS: u64 = 30;
const DEFAULT_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS: u64 = 1_000;
const DEFAULT_OBJECT_CLEANUP_BATCH_SIZE: i64 = 50;
const DEFAULT_JUDGE_DISPATCH_POLL_MILLISECONDS: u64 = 500;
const DEFAULT_JUDGE_DISPATCH_LEASE_SECONDS: u64 = 30;
const DEFAULT_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS: u64 = 1_000;
const DEFAULT_JUDGE_DISPATCH_BATCH_SIZE: i64 = 50;
const DEFAULT_JUDGE_DISPATCH_MAX_ATTEMPTS: i32 = 8;
const DEFAULT_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS: u64 = 60;
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

#[derive(Clone)]
pub struct AppConfig {
    pub deployment_mode: DeploymentMode,
    pub bind_address: SocketAddr,
    pub trusted_proxy_cidrs: Vec<IpNet>,
    pub metrics_token: Option<String>,
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_acquire_timeout: Duration,
    pub readiness_timeout: Duration,
    pub run_migrations: bool,
    pub session_ttl: Duration,
    pub secure_cookies: bool,
    /// Login failures a single source address may accumulate per five-minute
    /// window. Configurable because the right value depends on the venue: the
    /// whole room usually shares one NAT address, so a value sized for one
    /// client locks out everybody.
    pub login_attempt_limit_per_ip: i64,
    /// Login failures a single account may accumulate per five-minute window,
    /// regardless of source address. This is the window that bounds guessing.
    pub login_account_attempt_limit: i64,
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
    pub redis_operation_timeout: Duration,
    /// Number of multiplexed Redis connections behind the shared handle.
    pub redis_pool_size: usize,
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
    pub object_storage_upload_timeout: Duration,
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
    pub judge_stuck_requeue_interval: std::time::Duration,
    pub judge_result_prefetch: u16,
    pub judge_result_reconnect_delay: Duration,
    pub cups_enabled: bool,
    pub cups_printer: String,
    pub cups_command_timeout: Duration,
}

impl std::fmt::Debug for AppConfig {
    /// Redacts everything that carries a credential. This struct is logged
    /// during startup, and the derived `Debug` printed the database password,
    /// the S3 keys, the CSRF secret, and the broker credentials with it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const REDACTED: &str = "<redacted>";
        f.debug_struct("AppConfig")
            .field("deployment_mode", &self.deployment_mode)
            .field("bind_address", &self.bind_address)
            .field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs)
            .field("metrics_token_configured", &self.metrics_token.is_some())
            .field("database_url", &REDACTED)
            .field("database_max_connections", &self.database_max_connections)
            .field("database_acquire_timeout", &self.database_acquire_timeout)
            .field("readiness_timeout", &self.readiness_timeout)
            .field("run_migrations", &self.run_migrations)
            .field("session_ttl", &self.session_ttl)
            .field("secure_cookies", &self.secure_cookies)
            .field("login_attempt_limit_per_ip", &self.login_attempt_limit_per_ip)
            .field("login_account_attempt_limit", &self.login_account_attempt_limit)
            .field("csrf_secret", &REDACTED)
            .field("uses_development_csrf_secret", &self.uses_development_csrf_secret)
            .field("allow_development_csrf_secret", &self.allow_development_csrf_secret)
            .field("realtime_dispatcher_enabled", &self.realtime_dispatcher_enabled)
            .field("realtime_channel_capacity", &self.realtime_channel_capacity)
            .field("realtime_poll_interval", &self.realtime_poll_interval)
            .field("realtime_lease", &self.realtime_lease)
            .field("realtime_retry_base", &self.realtime_retry_base)
            .field("realtime_batch_size", &self.realtime_batch_size)
            .field("realtime_max_attempts", &self.realtime_max_attempts)
            .field("realtime_redis_enabled", &self.realtime_redis_enabled)
            .field("redis_url", &REDACTED)
            .field("redis_operation_timeout", &self.redis_operation_timeout)
            .field("redis_pool_size", &self.redis_pool_size)
            .field("realtime_redis_channel", &self.realtime_redis_channel)
            .field("realtime_redis_reconnect_delay", &self.realtime_redis_reconnect_delay)
            .field("scoreboard_cache_enabled", &self.scoreboard_cache_enabled)
            .field("scoreboard_cache_ttl", &self.scoreboard_cache_ttl)
            .field("scoreboard_cache_timeout", &self.scoreboard_cache_timeout)
            .field("object_storage_enabled", &self.object_storage_enabled)
            .field("object_storage_endpoint", &self.object_storage_endpoint)
            .field("object_storage_region", &self.object_storage_region)
            .field("object_storage_access_key", &REDACTED)
            .field("object_storage_secret_key", &REDACTED)
            .field("object_storage_problem_bucket", &self.object_storage_problem_bucket)
            .field("object_storage_source_bucket", &self.object_storage_source_bucket)
            .field("object_storage_force_path_style", &self.object_storage_force_path_style)
            .field("object_storage_request_timeout", &self.object_storage_request_timeout)
            .field("object_storage_upload_timeout", &self.object_storage_upload_timeout)
            .field("object_cleanup_poll_interval", &self.object_cleanup_poll_interval)
            .field("object_cleanup_lease", &self.object_cleanup_lease)
            .field("object_cleanup_retry_base", &self.object_cleanup_retry_base)
            .field("object_cleanup_batch_size", &self.object_cleanup_batch_size)
            .field("rabbitmq_enabled", &self.rabbitmq_enabled)
            .field("rabbitmq_url", &REDACTED)
            .field("rabbitmq_request_timeout", &self.rabbitmq_request_timeout)
            .field("judge_dispatch_poll_interval", &self.judge_dispatch_poll_interval)
            .field("judge_dispatch_lease", &self.judge_dispatch_lease)
            .field("judge_dispatch_retry_base", &self.judge_dispatch_retry_base)
            .field("judge_dispatch_batch_size", &self.judge_dispatch_batch_size)
            .field("judge_dispatch_max_attempts", &self.judge_dispatch_max_attempts)
            .field("judge_stuck_requeue_interval", &self.judge_stuck_requeue_interval)
            .field("judge_result_prefetch", &self.judge_result_prefetch)
            .field("judge_result_reconnect_delay", &self.judge_result_reconnect_delay)
            .field("cups_enabled", &self.cups_enabled)
            .field("cups_printer", &self.cups_printer)
            .field("cups_command_timeout", &self.cups_command_timeout)
            .finish()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{name} has invalid value {value:?}: {reason}")]
    Invalid { name: &'static str, value: String, reason: &'static str },
}

mod from_env;

#[cfg(test)]
mod tests;
