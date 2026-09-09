use std::net::IpAddr;

use time::OffsetDateTime;

use crate::error::AppError;
use crate::features::competition::model::{CompetitionSessionResponse, WorkstationLoginGrant};

use crate::features::auth::service::{
    AuthService, AuthenticatedSession, LOGIN_ATTEMPT_LIMIT, LoginOutcome,
    crypto::{
        access_fingerprint, constant_time_equal, digest, invalid_credentials, not_authenticated,
        random_token, rate_limited,
    },
    internal::record_audit,
    store::{self, SessionRecord, timestamp_millis, workstation_ttl_seconds},
};
use crate::features::auth::{model::LoginRequest, password};

impl AuthService {
    pub async fn login(
        &self,
        request: LoginRequest,
        request_ip: IpAddr,
    ) -> Result<LoginOutcome, AppError> {
        request.validate()?;
        let request_ip = request_ip.to_string();
        let username = request.username;
        let password = request.password;

        if self.failed_login_count(&request_ip).await? >= LOGIN_ATTEMPT_LIMIT {
            return Err(AppError::too_many_requests(
                "RATE_LIMIT_EXCEEDED",
                "Too many login attempts; try again later",
            ));
        }

        let Some(row) = self.load_user_by_username(&username).await? else {
            password::verify_dummy(password)
                .await
                .map_err(|error| AppError::internal("dummy password verification failed", error))?;
            if !self.record_failed_login(&username, &request_ip, LOGIN_ATTEMPT_LIMIT).await? {
                return Err(rate_limited());
            }
            return Err(invalid_credentials());
        };

        let password_matches = password::verify(password.clone(), row.password_hash.clone())
            .await
            .map_err(|error| AppError::internal("password verification failed", error))?;
        if !password_matches || !row.enabled {
            if !self.record_failed_login(&username, &request_ip, LOGIN_ATTEMPT_LIMIT).await? {
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
        let ttl_seconds = u64::try_from(self.session_ttl.as_secs())
            .map_err(|error| AppError::internal("session TTL is too large", error))?;

        // PostgreSQL remains authoritative for the user record and the audit
        // trail; the session itself lives only in Redis, so the transaction
        // no longer carries any `auth_sessions` writes and expiry is enforced
        // by the Redis TTL instead of an `expires_at` column.
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
            if !self.record_failed_login(&username, &request_ip, LOGIN_ATTEMPT_LIMIT).await? {
                return Err(rate_limited());
            }
            return Err(invalid_credentials());
        }

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

        let now = OffsetDateTime::now_utc();
        let record = SessionRecord {
            user_id: user.id,
            access_fingerprint,
            created_at_millis: timestamp_millis(now),
            last_seen_millis: timestamp_millis(now),
            workstation_binding_id: None,
            bound_ip: None,
        };
        store::create_session(self.redis()?, &session_token_hash, &record, ttl_seconds).await?;

        Ok(LoginOutcome { user, session_token })
    }
    pub async fn authenticate(
        &self,
        session_token: &str,
    ) -> Result<AuthenticatedSession, AppError> {
        if session_token.is_empty() || session_token.len() > 256 {
            return Err(not_authenticated());
        }
        let token_hash = digest(session_token);
        let redis = self.redis()?;
        let Some(session) = store::load_session(redis, &token_hash).await? else {
            return Err(not_authenticated());
        };
        let SessionRecord {
            user_id,
            access_fingerprint: stored_fingerprint,
            last_seen_millis,
            workstation_binding_id,
            bound_ip,
            ..
        } = session;

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

        // Refresh last_seen at most once per five minutes, preserving the
        // remaining session TTL (sessions keep their fixed expiry).
        let now_millis = timestamp_millis(OffsetDateTime::now_utc());
        if now_millis - last_seen_millis > 5 * 60 * 1000 {
            if let Some(mut refreshed) = store::load_session(redis, &token_hash).await? {
                refreshed.last_seen_millis = now_millis;
                store::touch_session(redis, &token_hash, &refreshed).await?;
            }
        }

        Ok(AuthenticatedSession {
            user,
            token_hash,
            workstation_binding_id,
            bound_ip,
            competition: None,
        })
    }
    pub async fn create_workstation_session(
        &self,
        grant: WorkstationLoginGrant,
    ) -> Result<(LoginOutcome, CompetitionSessionResponse), AppError> {
        let Some(row) = self.load_user_by_id(grant.user_id).await? else {
            return Err(not_authenticated());
        };
        if !row.enabled || row.user_type != "TEAM" {
            return Err(not_authenticated());
        }
        let user = row.auth_user()?;
        let session_token = random_token()?;
        let token_hash = digest(&session_token);
        let access_fingerprint = access_fingerprint(&user);
        let ttl_seconds = workstation_ttl_seconds(grant.expires_at, self.session_ttl.as_secs());
        let now = OffsetDateTime::now_utc();
        let record = SessionRecord {
            user_id: user.id,
            access_fingerprint,
            created_at_millis: timestamp_millis(now),
            last_seen_millis: timestamp_millis(now),
            workstation_binding_id: Some(grant.binding_id),
            bound_ip: Some(grant.bound_ip.clone()),
        };
        store::create_workstation_session(self.redis()?, &token_hash, &record, ttl_seconds)
            .await?;
        Ok((LoginOutcome { user, session_token }, grant.competition))
    }
    pub async fn logout(&self, token_hash: &str) -> Result<(), AppError> {
        self.delete_session(token_hash).await
    }
    pub async fn logout_token(&self, raw_token: &str) -> Result<(), AppError> {
        self.delete_session(&digest(raw_token)).await
    }
}
