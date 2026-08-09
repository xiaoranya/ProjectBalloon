use std::time::Duration;

use sqlx::PgPool;

use crate::features::auth::model::AuthUser;
use crate::features::competition::model::CompetitionSessionResponse;

mod account;
mod crypto;
mod internal;
mod sessions;

#[cfg(test)]
mod tests;

const LOGIN_ATTEMPT_LIMIT: i64 = 10;
const REGISTER_ATTEMPT_LIMIT: i64 = 5;
const PASSWORD_CHANGE_ATTEMPT_LIMIT: i64 = 10;
const PROFILE_UPDATE_ATTEMPT_LIMIT: i64 = 30;
const SESSION_TOKEN_BYTES: usize = 32;

const USER_COLUMNS: &str = r#"
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
    session_ttl: Duration,
    secure_cookies: bool,
}

impl AuthService {
    #[must_use]
    pub const fn new(database: PgPool, session_ttl: Duration, secure_cookies: bool) -> Self {
        Self { database, session_ttl, secure_cookies }
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
