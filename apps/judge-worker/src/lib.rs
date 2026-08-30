pub mod artifacts;
pub mod health;
pub mod heartbeat;
pub mod rabbit;
pub mod sandbox;
pub mod worker;

use std::{env, path::PathBuf, time::Duration};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub cache_dir: PathBuf,
    pub task_queue: String,
    pub amqp_url: String,
    pub task_prefetch: u16,
    pub reconnect_delay: Duration,
    pub request_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub object_storage_endpoint: String,
    pub object_storage_region: String,
    pub object_storage_access_key: String,
    pub object_storage_secret_key: String,
    pub problem_bucket: String,
    pub source_bucket: String,
    pub max_artifact_bytes: u64,
    pub sandbox_socket: PathBuf,
    pub sandbox_runtime: Option<String>,
    pub sandbox_user: String,
    pub c_image: String,
    pub cpp_image: String,
    pub java_image: String,
    pub python_image: String,
    pub health_port: u16,
    pub health_session_error_window: Duration,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("{name} must not be empty")]
    Empty { name: &'static str },
    #[error("{name} contains unsupported control characters")]
    ControlCharacter { name: &'static str },
    #[error("{name} has invalid value: {reason}")]
    Invalid { name: &'static str, reason: &'static str },
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let worker_id = lookup("WORKER_ID").unwrap_or_else(|| "worker-local".to_owned());
        let cache_dir = lookup("JUDGE_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/cache/judge"));
        let task_queue = lookup("JUDGE_TASK_QUEUE").unwrap_or_else(|| "judge.tasks".to_owned());
        let amqp_url = lookup("PROJECT_BALLOON_RABBITMQ_URL").unwrap_or_default();
        let task_prefetch = parse_positive(
            "JUDGE_TASK_PREFETCH",
            lookup("JUDGE_TASK_PREFETCH").unwrap_or_else(|| "1".to_owned()),
        )?;
        let reconnect_milliseconds = parse_positive(
            "JUDGE_RECONNECT_MILLISECONDS",
            lookup("JUDGE_RECONNECT_MILLISECONDS").unwrap_or_else(|| "1000".to_owned()),
        )?;
        let request_timeout_milliseconds = parse_positive(
            "JUDGE_REQUEST_TIMEOUT_MILLISECONDS",
            lookup("JUDGE_REQUEST_TIMEOUT_MILLISECONDS").unwrap_or_else(|| "10000".to_owned()),
        )?;
        let heartbeat_interval_seconds = parse_positive(
            "JUDGE_HEARTBEAT_INTERVAL_SECONDS",
            lookup("JUDGE_HEARTBEAT_INTERVAL_SECONDS").unwrap_or_else(|| "5".to_owned()),
        )?;
        let object_storage_endpoint = lookup("PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT")
            .unwrap_or_else(|| "http://127.0.0.1:9000".to_owned());
        let object_storage_region = lookup("PROJECT_BALLOON_OBJECT_STORAGE_REGION")
            .unwrap_or_else(|| "us-east-1".to_owned());
        let object_storage_access_key =
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY").unwrap_or_default();
        let object_storage_secret_key =
            lookup("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY").unwrap_or_default();
        let problem_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET")
            .unwrap_or_else(|| "xcpc-problems".to_owned());
        let source_bucket = lookup("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET")
            .unwrap_or_else(|| "xcpc-sources".to_owned());
        let max_artifact_bytes = parse_positive(
            "JUDGE_MAX_ARTIFACT_BYTES",
            lookup("JUDGE_MAX_ARTIFACT_BYTES")
                .unwrap_or_else(|| (300_u64 * 1024 * 1024).to_string()),
        )?;
        let sandbox_socket = lookup("XCPC_SANDBOX_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"));
        let sandbox_runtime =
            lookup("XCPC_SANDBOX_RUNTIME").filter(|value| !value.trim().is_empty());
        let sandbox_user = lookup("XCPC_SANDBOX_USER").unwrap_or_else(|| "1000:1000".to_owned());
        let c_image =
            lookup("JUDGE_C_IMAGE").unwrap_or_else(|| "judge-runtime-c:12.2.0".to_owned());
        let cpp_image =
            lookup("JUDGE_CPP_IMAGE").unwrap_or_else(|| "judge-runtime-cpp:12.2.0".to_owned());
        let java_image =
            lookup("JUDGE_JAVA_IMAGE").unwrap_or_else(|| "judge-runtime-java:21".to_owned());
        let python_image = lookup("JUDGE_PYTHON_IMAGE")
            .unwrap_or_else(|| "judge-runtime-python:3.12.13".to_owned());
        let health_port = parse_positive::<u64>(
            "JUDGE_HEALTH_PORT",
            lookup("JUDGE_HEALTH_PORT").unwrap_or_else(|| "9101".to_owned()),
        )?;
        let health_port = u16::try_from(health_port).map_err(|_| ConfigError::Invalid {
            name: "JUDGE_HEALTH_PORT",
            reason: "expected a value between 1 and 65535",
        })?;
        let health_session_error_window_seconds = parse_positive(
            "JUDGE_HEALTH_SESSION_ERROR_WINDOW_SECONDS",
            lookup("JUDGE_HEALTH_SESSION_ERROR_WINDOW_SECONDS").unwrap_or_else(|| "60".to_owned()),
        )?;

        validate_text("WORKER_ID", &worker_id)?;
        validate_text("JUDGE_TASK_QUEUE", &task_queue)?;
        validate_sandbox_user(&sandbox_user)?;
        for (name, value) in [
            ("PROJECT_BALLOON_OBJECT_STORAGE_REGION", &object_storage_region),
            ("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY", &object_storage_access_key),
            ("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY", &object_storage_secret_key),
            ("PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET", &problem_bucket),
            ("PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET", &source_bucket),
            ("XCPC_SANDBOX_USER", &sandbox_user),
            ("JUDGE_C_IMAGE", &c_image),
            ("JUDGE_CPP_IMAGE", &cpp_image),
            ("JUDGE_JAVA_IMAGE", &java_image),
            ("JUDGE_PYTHON_IMAGE", &python_image),
        ] {
            validate_text(name, value)?;
        }
        if !amqp_url.starts_with("amqp://") && !amqp_url.starts_with("amqps://") {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_RABBITMQ_URL",
                reason: "expected an AMQP or AMQPS URL",
            });
        }
        if !object_storage_endpoint.starts_with("http://")
            && !object_storage_endpoint.starts_with("https://")
        {
            return Err(ConfigError::Invalid {
                name: "PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT",
                reason: "expected an HTTP or HTTPS URL",
            });
        }

        Ok(Self {
            worker_id,
            cache_dir,
            task_queue,
            amqp_url,
            task_prefetch,
            reconnect_delay: Duration::from_millis(reconnect_milliseconds),
            request_timeout: Duration::from_millis(request_timeout_milliseconds),
            heartbeat_interval: Duration::from_secs(heartbeat_interval_seconds),
            object_storage_endpoint,
            object_storage_region,
            object_storage_access_key,
            object_storage_secret_key,
            problem_bucket,
            source_bucket,
            max_artifact_bytes,
            sandbox_socket,
            sandbox_runtime,
            sandbox_user,
            c_image,
            cpp_image,
            java_image,
            python_image,
            health_port,
            health_session_error_window: Duration::from_secs(health_session_error_window_seconds),
        })
    }
}

fn parse_positive<T>(name: &'static str, value: String) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let parsed = value
        .parse()
        .map_err(|_| ConfigError::Invalid { name, reason: "expected a positive integer" })?;
    if parsed <= T::from(0) {
        return Err(ConfigError::Invalid { name, reason: "must be greater than zero" });
    }
    Ok(parsed)
}

fn validate_text(name: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::Empty { name });
    }
    if value.chars().any(char::is_control) {
        return Err(ConfigError::ControlCharacter { name });
    }
    if name == "WORKER_ID" && value.len() > 64 {
        return Err(ConfigError::Invalid { name, reason: "must contain at most 64 bytes" });
    }
    Ok(())
}

fn validate_sandbox_user(value: &str) -> Result<(), ConfigError> {
    let uid = value.trim().split(':').next().unwrap_or_default();
    if uid == "0" || uid.eq_ignore_ascii_case("root") {
        return Err(ConfigError::Invalid { name: "XCPC_SANDBOX_USER", reason: "must not be root" });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{ConfigError, WorkerConfig};

    #[test]
    fn configuration_accepts_explicit_infrastructure_credentials() {
        let values = HashMap::from([
            ("PROJECT_BALLOON_RABBITMQ_URL", "amqp://worker:secret@127.0.0.1:5672/%2f".to_owned()),
            ("PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY", "worker-access".to_owned()),
            ("PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY", "worker-secret".to_owned()),
        ]);
        let config = WorkerConfig::from_lookup(|name| values.get(name).cloned())
            .expect("explicit infrastructure credentials must be valid");

        assert_eq!(config.worker_id, "worker-local");
        assert_eq!(config.cache_dir.to_string_lossy(), "/var/cache/judge");
        assert_eq!(config.task_queue, "judge.tasks");
        assert_eq!(config.task_prefetch, 1);
        assert_eq!(config.heartbeat_interval, std::time::Duration::from_secs(5));
        assert_eq!(config.max_artifact_bytes, 300 * 1024 * 1024);
        assert_eq!(config.cpp_image, "judge-runtime-cpp:12.2.0");
        assert_eq!(config.java_image, "judge-runtime-java:21");
        assert_eq!(config.python_image, "judge-runtime-python:3.12.13");
    }

    #[test]
    fn configuration_rejects_empty_worker_id() {
        let values = HashMap::from([("WORKER_ID", "  ".to_owned())]);
        let error = WorkerConfig::from_lookup(|name| values.get(name).cloned())
            .expect_err("empty worker ID must fail");

        assert_eq!(error, ConfigError::Empty { name: "WORKER_ID" });
    }

    #[test]
    fn configuration_rejects_worker_id_longer_than_protocol_limit() {
        let values = HashMap::from([("WORKER_ID", "x".repeat(65))]);
        let error =
            WorkerConfig::from_lookup(|name| values.get(name).cloned()).expect_err("invalid");
        assert_eq!(
            error,
            ConfigError::Invalid { name: "WORKER_ID", reason: "must contain at most 64 bytes" }
        );
    }

    #[test]
    fn configuration_rejects_root_sandbox_user() {
        let values = HashMap::from([("XCPC_SANDBOX_USER", "0:0".to_owned())]);
        let error =
            WorkerConfig::from_lookup(|name| values.get(name).cloned()).expect_err("invalid");
        assert_eq!(
            error,
            ConfigError::Invalid { name: "XCPC_SANDBOX_USER", reason: "must not be root" }
        );
    }
}
