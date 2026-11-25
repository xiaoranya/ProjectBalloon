use std::time::Duration;

use sqlx::PgPool;

use crate::error::AppError;
use crate::features::auth::model::AuthUser;
use crate::features::competition::model::CompetitionSessionResponse;
use crate::features::redis::RedisHandle;

mod account;
mod crypto;
mod internal;
mod sessions;
mod store;

pub(crate) use store::{revoke_user_sessions, revoke_workstation_sessions};

#[cfg(test)]
mod tests;

/// Compiled-in default for the per-source-IP login window; deployments
/// override it with `PROJECT_BALLOON_LOGIN_ATTEMPT_LIMIT_PER_IP`. Sized for a
/// venue where hundreds of teams share one NAT address: the pre-audit value of
/// ten locked the whole room out on ordinary typos. Guessing is bounded by the
/// per-account window, which is the one that matters.
pub(crate) const LOGIN_ATTEMPT_LIMIT: i64 = 100;
/// Compiled-in default for the per-account login window (see `internal`).
pub(crate) const LOGIN_ACCOUNT_ATTEMPT_LIMIT: i64 = 10;

/// The two login windows, both enforced in Redis. A shared venue NAT makes the
/// per-IP window a capacity question rather than a security one, so it is
/// configured per deployment instead of being hard-coded.
#[derive(Debug, Clone, Copy)]
pub struct LoginRateLimits {
    pub per_ip: i64,
    pub per_account: i64,
}

impl Default for LoginRateLimits {
    fn default() -> Self {
        Self { per_ip: LOGIN_ATTEMPT_LIMIT, per_account: LOGIN_ACCOUNT_ATTEMPT_LIMIT }
    }
}
const REGISTER_ATTEMPT_LIMIT: i64 = 5;
const PASSWORD_CHANGE_ATTEMPT_LIMIT: i64 = 10;
const PROFILE_UPDATE_ATTEMPT_LIMIT: i64 = 30;
const SESSION_TOKEN_BYTES: usize = 32;

pub(crate) const USER_COLUMNS: &str = r#"
    u.id,
    u.username,
    u.password_hash,
    u.display_name,
    u.user_type,
    u.enabled,
    u.password_reset_required,
    COALESCE(
        array_agg(p.code ORDER BY p.code) FILTER (WHERE p.code IS NOT NULL),
        ARRAY[]::varchar[]
    ) AS permissions
"#;

pub struct LoginOutcome {
    pub user: AuthUser,
    pub session_token: String,
}

#[derive(Debug)]
pub struct AuthenticatedSession {
    pub user: AuthUser,
    pub token_hash: String,
    pub workstation_binding_id: Option<i64>,
    pub bound_ip: Option<String>,
    pub competition: Option<CompetitionSessionResponse>,
}

pub struct AuthService {
    database: PgPool,
    redis: Option<RedisHandle>,
    session_ttl: Duration,
    secure_cookies: bool,
    login_limits: LoginRateLimits,
}

impl AuthService {
    #[must_use]
    pub const fn new(database: PgPool, session_ttl: Duration, secure_cookies: bool) -> Self {
        Self {
            database,
            redis: None,
            session_ttl,
            secure_cookies,
            login_limits: LoginRateLimits {
                per_ip: LOGIN_ATTEMPT_LIMIT,
                per_account: LOGIN_ACCOUNT_ATTEMPT_LIMIT,
            },
        }
    }

    /// Overrides the two login windows with the deployment's configuration.
    #[must_use]
    pub const fn with_login_rate_limits(mut self, limits: LoginRateLimits) -> Self {
        self.login_limits = limits;
        self
    }

    /// Wires the shared Redis handle. Sessions and login rate limiting live
    /// exclusively in Redis; without a handle authentication is unavailable.
    #[must_use]
    pub fn with_redis_option(mut self, redis: Option<RedisHandle>) -> Self {
        self.redis = redis;
        self
    }

    pub(crate) fn redis(&self) -> Result<&RedisHandle, AppError> {
        self.redis.as_ref().ok_or_else(|| {
            AppError::internal_message(
                "redis session store unavailable",
                "the API process was started without a Redis connection",
            )
        })
    }

    #[must_use]
    pub const fn session_ttl(&self) -> Duration {
        self.session_ttl
    }

    #[must_use]
    pub const fn secure_cookies(&self) -> bool {
        self.secure_cookies
    }
}
