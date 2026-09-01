use std::{collections::HashMap, time::Duration};

use crate::config::{AppConfig, ConfigError, DeploymentMode};

/// Wraps a value map so the development CSRF secret is explicitly allowed;
/// every validation test below targets a different concern, not the CSRF
/// default-secret rejection.
fn dev_lookup<'a>(values: &'a HashMap<&'a str, String>) -> impl FnMut(&str) -> Option<String> + 'a {
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
    assert_eq!(config.object_storage_request_timeout.as_millis(), 5_000);
    assert_eq!(config.object_storage_upload_timeout.as_millis(), 300_000);
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
fn metrics_token_defaults_to_open_and_accepts_only_header_safe_values() {
    let defaults = AppConfig::from_lookup(dev_lookup(&HashMap::new())).expect("defaults");
    assert!(defaults.metrics_token.is_none(), "/metrics stays open by default");

    let values = HashMap::from([("PROJECT_BALLOON_METRICS_TOKEN", "  s3cr3t ".to_owned())]);
    let config = AppConfig::from_lookup(dev_lookup(&values)).expect("metrics token");
    assert_eq!(config.metrics_token.as_deref(), Some("s3cr3t"));

    let empty = HashMap::from([("PROJECT_BALLOON_METRICS_TOKEN", "   ".to_owned())]);
    let config = AppConfig::from_lookup(dev_lookup(&empty)).expect("empty token disables auth");
    assert!(config.metrics_token.is_none());

    let control = HashMap::from([("PROJECT_BALLOON_METRICS_TOKEN", "bad\nvalue".to_owned())]);
    assert!(matches!(
        AppConfig::from_lookup(dev_lookup(&control)),
        Err(ConfigError::Invalid { name: "PROJECT_BALLOON_METRICS_TOKEN", .. })
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
fn object_storage_upload_timeout_is_independently_configured() {
    let values = HashMap::from([(
        "PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS",
        "600000".to_owned(),
    )]);
    let config = AppConfig::from_lookup(dev_lookup(&values)).expect("custom upload timeout");
    assert_eq!(config.object_storage_upload_timeout.as_millis(), 600_000);
    // The metadata request budget stays at its default.
    assert_eq!(config.object_storage_request_timeout.as_millis(), 5_000);
}

#[test]
fn object_storage_upload_timeout_must_be_positive() {
    let values = HashMap::from([(
        "PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS",
        "0".to_owned(),
    )]);
    let error = AppConfig::from_lookup(dev_lookup(&values))
        .expect_err("zero upload timeout must be rejected");
    assert_eq!(
        error,
        ConfigError::Invalid {
            name: "PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS",
            value: "0".to_owned(),
            reason: "must be greater than zero",
        }
    );
}

#[test]
fn enabled_rabbitmq_requires_an_amqp_url() {
    let values = HashMap::from([
        ("PROJECT_BALLOON_RABBITMQ_ENABLED", "true".to_owned()),
        ("PROJECT_BALLOON_RABBITMQ_URL", "http://rabbit.invalid".to_owned()),
    ]);
    assert!(AppConfig::from_lookup(dev_lookup(&values)).is_err());
}
