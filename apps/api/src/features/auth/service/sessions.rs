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
            if !self.record_failed_login(&username, &request_ip, LOGIN_ATTEMPT_LIMIT).await? {
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
    pub async fn authenticate(
        &self,
        session_token: &str,
    ) -> Result<AuthenticatedSession, AppError> {
        if session_token.is_empty() || session_token.len() > 256 {
            return Err(not_authenticated());
        }
        let token_hash = digest(session_token);
        let session = sqlx::query_as::<_, (i64, String, Option<i64>, Option<String>)>(
            r#"
            SELECT user_id, access_fingerprint, workstation_binding_id, bound_ip
            FROM auth_sessions
            WHERE token_hash = $1 AND expires_at > now()
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load authentication session", error))?;
        let Some((user_id, stored_fingerprint, workstation_binding_id, bound_ip)) = session else {
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
        let ttl_seconds = i64::try_from(self.session_ttl.as_secs())
            .map_err(|error| AppError::internal("session TTL is too large", error))?;
        let expires_at = std::cmp::min(
            grant.expires_at,
            OffsetDateTime::now_utc() + time::Duration::seconds(ttl_seconds),
        );
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin workstation login", error))?;
        sqlx::query("DELETE FROM auth_sessions WHERE workstation_binding_id=$1")
            .bind(grant.binding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("replace workstation session", error))?;
        sqlx::query(
            r#"
            INSERT INTO auth_sessions
                (token_hash,user_id,access_fingerprint,expires_at,workstation_binding_id,bound_ip)
            VALUES($1,$2,$3,$4,$5,$6)
            "#,
        )
        .bind(&token_hash)
        .bind(user.id)
        .bind(access_fingerprint)
        .bind(expires_at)
        .bind(grant.binding_id)
        .bind(grant.bound_ip)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("create workstation session", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit workstation login", error))?;
        Ok((LoginOutcome { user, session_token }, grant.competition))
    }
    pub async fn logout(&self, token_hash: &str) -> Result<(), AppError> {
        self.delete_session(token_hash).await
    }
    pub async fn logout_token(&self, raw_token: &str) -> Result<(), AppError> {
        self.delete_session(&digest(raw_token)).await
    }
}
