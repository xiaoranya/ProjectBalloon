use std::{net::IpAddr, time::Duration};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use subtle::ConstantTimeEq;

use crate::error::AppError;

use super::{
    model::{AuthUser, ChangePasswordRequest, LoginRequest, RegisterRequest, UserRow},
    password,
};

const LOGIN_ATTEMPT_LIMIT: i64 = 10;
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
        array_agg(r.code ORDER BY r.code) FILTER (WHERE r.code IS NOT NULL),
        ARRAY[]::varchar[]
    ) AS roles
"#;

pub struct LoginOutcome {
    pub user: AuthUser,
    pub session_token: String,
}

#[derive(Debug)]
pub struct AuthenticatedSession {
    pub user: AuthUser,
    pub token_hash: String,
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

    pub async fn login(
        &self,
        request: LoginRequest,
        request_ip: IpAddr,
    ) -> Result<LoginOutcome, AppError> {
        request.validate()?;
        let request_ip = request_ip.to_string();
        let username = request.username;
        let password = request.password;

        if self.failed_login_count(&username, &request_ip).await? >= LOGIN_ATTEMPT_LIMIT {
            return Err(AppError::too_many_requests(
                "RATE_LIMIT_EXCEEDED",
                "Too many login attempts; try again later",
            ));
        }

        let Some(row) = self.load_user_by_username(&username).await? else {
            password::hash(password)
                .await
                .map_err(|error| AppError::internal("dummy password hashing failed", error))?;
            if !self.record_failed_login(&username, &request_ip).await? {
                return Err(rate_limited());
            }
            return Err(invalid_credentials());
        };

        let password_matches = password::verify(password.clone(), row.password_hash.clone())
            .await
            .map_err(|error| AppError::internal("password verification failed", error))?;
        if !password_matches || !row.enabled {
            if !self.record_failed_login(&username, &request_ip).await? {
                return Err(rate_limited());
            }
            return Err(invalid_credentials());
        }

        let upgraded_hash = if password::needs_upgrade(&row.password_hash) {
            Some(
                password::hash(password)
                    .await
                    .map_err(|error| AppError::internal("password upgrade failed", error))?,
            )
        } else {
            None
        };
        let user = row.auth_user()?;
        let session_token = random_token()?;
        let session_token_hash = digest(&session_token);
        let access_fingerprint = access_fingerprint(&user);
        let session_ttl_seconds = i64::try_from(self.session_ttl.as_secs())
            .map_err(|error| AppError::internal("session TTL is too large", error))?;

        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin login transaction", error))?;

        let update = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = COALESCE($1, password_hash),
                last_login_at = now(),
                updated_at = now()
            WHERE id = $2 AND enabled = true AND password_hash = $3
            "#,
        )
        .bind(upgraded_hash)
        .bind(user.id)
        .bind(&row.password_hash)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("update successful login", error))?;

        if update.rows_affected() != 1 {
            transaction
                .rollback()
                .await
                .map_err(|error| AppError::internal("rollback stale login", error))?;
            if !self.record_failed_login(&username, &request_ip).await? {
                return Err(rate_limited());
            }
            return Err(invalid_credentials());
        }

        sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= now()")
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("clean expired sessions", error))?;
        sqlx::query(
            r#"
            INSERT INTO auth_sessions (token_hash, user_id, access_fingerprint, expires_at)
            VALUES ($1, $2, $3, now() + ($4 * interval '1 second'))
            "#,
        )
        .bind(&session_token_hash)
        .bind(user.id)
        .bind(access_fingerprint)
        .bind(session_ttl_seconds)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("create login session", error))?;
        record_audit(
            &mut transaction,
            Some(user.id),
            "auth.login",
            &user.id.to_string(),
            &request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit login transaction", error))?;

        Ok(LoginOutcome { user, session_token })
    }

    pub async fn register(
        &self,
        request: RegisterRequest,
        request_ip: IpAddr,
    ) -> Result<LoginOutcome, AppError> {
        request.validate()?;
        let username = request.username.trim().to_owned();
        let display_name = request.display_name.trim().to_owned();
        let password_hash = password::hash(request.password.clone())
            .await
            .map_err(|e| AppError::internal("hash registration password", e))?;
        let inserted=sqlx::query("INSERT INTO users(username,password_hash,display_name,user_type) VALUES($1,$2,$3,'INDIVIDUAL')")
            .bind(&username).bind(password_hash).bind(&display_name).execute(&self.database).await;
        match inserted {
            Ok(_) => {
                self.login(LoginRequest { username, password: request.password }, request_ip).await
            }
            Err(sqlx::Error::Database(error)) if error.constraint().is_some() => {
                Err(AppError::conflict("USERNAME_TAKEN", "Username is already registered"))
            }
            Err(error) => Err(AppError::internal("create individual account", error)),
        }
    }

    pub async fn authenticate(
        &self,
        session_token: &str,
    ) -> Result<AuthenticatedSession, AppError> {
        if session_token.is_empty() || session_token.len() > 256 {
            return Err(not_authenticated());
        }
        let token_hash = digest(session_token);
        let session = sqlx::query_as::<_, (i64, String)>(
            r#"
            SELECT user_id, access_fingerprint
            FROM auth_sessions
            WHERE token_hash = $1 AND expires_at > now()
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load authentication session", error))?;
        let Some((user_id, stored_fingerprint)) = session else {
            return Err(not_authenticated());
        };

        let Some(row) = self.load_user_by_id(user_id).await? else {
            self.delete_session(&token_hash).await?;
            return Err(not_authenticated());
        };
        if !row.enabled {
            self.delete_session(&token_hash).await?;
            return Err(AppError::unauthorized("ACCOUNT_DISABLED", "Account is disabled"));
        }
        let user = row.auth_user()?;
        let current_fingerprint = access_fingerprint(&user);
        if !constant_time_equal(&stored_fingerprint, &current_fingerprint) {
            self.delete_session(&token_hash).await?;
            return Err(AppError::unauthorized(
                "ACCOUNT_ACCESS_CHANGED",
                "Account access changed; sign in again",
            ));
        }

        sqlx::query(
            r#"
            UPDATE auth_sessions
            SET last_seen_at = now()
            WHERE token_hash = $1 AND last_seen_at < now() - interval '5 minutes'
            "#,
        )
        .bind(&token_hash)
        .execute(&self.database)
        .await
        .map_err(|error| AppError::internal("refresh authentication session", error))?;

        Ok(AuthenticatedSession { user, token_hash })
    }

    pub async fn logout(&self, token_hash: &str) -> Result<(), AppError> {
        self.delete_session(token_hash).await
    }

    pub async fn logout_token(&self, raw_token: &str) -> Result<(), AppError> {
        self.delete_session(&digest(raw_token)).await
    }

    pub async fn change_password(
        &self,
        session: &AuthenticatedSession,
        request: ChangePasswordRequest,
        request_ip: IpAddr,
    ) -> Result<AuthUser, AppError> {
        request.validate()?;
        let stored_hash = sqlx::query_scalar::<_, String>(
            "SELECT password_hash FROM users WHERE id = $1 AND enabled = true",
        )
        .bind(session.user.id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load password for change", error))?
        .ok_or_else(|| AppError::unauthorized("ACCOUNT_DISABLED", "Account is disabled"))?;

        let current_matches = password::verify(request.current_password, stored_hash.clone())
            .await
            .map_err(|error| AppError::internal("verify current password", error))?;
        if !current_matches {
            return Err(AppError::bad_request(
                "CURRENT_PASSWORD_INVALID",
                "CURRENT_PASSWORD_INVALID",
            ));
        }
        let unchanged = password::verify(request.new_password.clone(), stored_hash.clone())
            .await
            .map_err(|error| AppError::internal("compare new password", error))?;
        if unchanged {
            return Err(AppError::bad_request("PASSWORD_UNCHANGED", "PASSWORD_UNCHANGED"));
        }
        let new_hash = password::hash(request.new_password)
            .await
            .map_err(|error| AppError::internal("hash new password", error))?;

        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin password transaction", error))?;
        let update = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $1,
                password_reset_required = false,
                updated_at = now()
            WHERE id = $2 AND enabled = true AND password_hash = $3
            "#,
        )
        .bind(new_hash)
        .bind(session.user.id)
        .bind(stored_hash)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("update password", error))?;
        if update.rows_affected() != 1 {
            return Err(AppError::bad_request(
                "CURRENT_PASSWORD_INVALID",
                "CURRENT_PASSWORD_INVALID",
            ));
        }
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1 AND token_hash <> $2")
            .bind(session.user.id)
            .bind(&session.token_hash)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("revoke other password sessions", error))?;
        record_audit(
            &mut transaction,
            Some(session.user.id),
            "PASSWORD_CHANGED",
            &session.user.id.to_string(),
            &request_ip.to_string(),
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit password transaction", error))?;

        let mut user = session.user.clone();
        user.password_reset_required = false;
        Ok(user)
    }

    async fn load_user_by_username(&self, username: &str) -> Result<Option<UserRow>, AppError> {
        let query = format!(
            r#"
            SELECT {USER_COLUMNS}
            FROM users u
            LEFT JOIN user_roles ur ON ur.user_id = u.id
            LEFT JOIN roles r ON r.id = ur.role_id
            WHERE u.username = $1
            GROUP BY u.id
            "#
        );
        sqlx::query_as::<_, UserRow>(&query)
            .bind(username)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load user by username", error))
    }

    async fn load_user_by_id(&self, user_id: i64) -> Result<Option<UserRow>, AppError> {
        let query = format!(
            r#"
            SELECT {USER_COLUMNS}
            FROM users u
            LEFT JOIN user_roles ur ON ur.user_id = u.id
            LEFT JOIN roles r ON r.id = ur.role_id
            WHERE u.id = $1
            GROUP BY u.id
            "#
        );
        sqlx::query_as::<_, UserRow>(&query)
            .bind(user_id)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load user by ID", error))
    }

    async fn failed_login_count(&self, username: &str, request_ip: &str) -> Result<i64, AppError> {
        sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM audit_logs
            WHERE action = 'auth.login'
              AND result = 'failed'
              AND target_id = $1
              AND request_ip = $2
              AND created_at > now() - interval '5 minutes'
            "#,
        )
        .bind(username.to_lowercase())
        .bind(request_ip)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check login rate limit", error))
    }

    async fn record_failed_login(
        &self,
        username: &str,
        request_ip: &str,
    ) -> Result<bool, AppError> {
        sqlx::query_scalar(
            r#"
            WITH locked AS (
                SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))
            ), attempts AS (
                SELECT count(*) AS count
                FROM audit_logs, locked
                WHERE action = 'auth.login'
                  AND result = 'failed'
                  AND target_id = $1
                  AND request_ip = $2
                  AND created_at > now() - interval '5 minutes'
            ), inserted AS (
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, request_ip, result)
            SELECT NULL, 'auth.login', 'user', $1, $2, 'failed'
            FROM attempts
            WHERE count < $3
            RETURNING id
            )
            SELECT EXISTS(SELECT 1 FROM inserted)
            "#,
        )
        .bind(username.to_lowercase())
        .bind(request_ip)
        .bind(LOGIN_ATTEMPT_LIMIT)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("record failed login", error))
    }

    async fn delete_session(&self, token_hash: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM auth_sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.database)
            .await
            .map(|_| ())
            .map_err(|error| AppError::internal("delete authentication session", error))
    }
}

async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<i64>,
    action: &str,
    target_id: &str,
    request_ip: &str,
    result: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES
            ($1, $2, 'user', $3, $4, $5)
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(target_id)
    .bind(request_ip)
    .bind(result)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record authentication audit", error))
}

fn random_token() -> Result<String, AppError> {
    let mut bytes = [0_u8; SESSION_TOKEN_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| AppError::internal("generate session token", error))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn access_fingerprint(user: &AuthUser) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user.user_type.as_str().as_bytes());
    for role in &user.roles {
        hasher.update([0]);
        hasher.update(role.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

fn invalid_credentials() -> AppError {
    AppError::unauthorized("INVALID_CREDENTIALS", "Invalid username or password")
}

fn rate_limited() -> AppError {
    AppError::too_many_requests("RATE_LIMIT_EXCEEDED", "Too many login attempts; try again later")
}

fn not_authenticated() -> AppError {
    AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated")
}

#[cfg(test)]
mod tests {
    use super::{access_fingerprint, constant_time_equal};
    use crate::features::auth::model::{AuthUser, UserType};

    #[test]
    fn access_fingerprint_is_deterministic() {
        let user = AuthUser {
            id: 1,
            username: "admin".to_owned(),
            display_name: "Admin".to_owned(),
            user_type: UserType::SuperAdmin,
            roles: vec!["JUDGE".to_owned(), "SUPER_ADMIN".to_owned()],
            password_reset_required: false,
        };

        assert_eq!(access_fingerprint(&user), access_fingerprint(&user));
        assert!(constant_time_equal("same", "same"));
        assert!(!constant_time_equal("same", "different"));
    }
}
