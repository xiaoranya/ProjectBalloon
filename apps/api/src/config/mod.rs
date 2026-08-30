use std::{net::SocketAddr, time::Duration};

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
    pub judge_stuck_requeue_interval: std::time::Duration,
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

mod from_env;

#[cfg(test)]
mod tests;
