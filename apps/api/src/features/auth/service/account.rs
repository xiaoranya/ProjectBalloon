use std::net::IpAddr;

use crate::error::AppError;

use crate::features::auth::service::{
    AuthService, AuthenticatedSession, LoginOutcome, PASSWORD_CHANGE_ATTEMPT_LIMIT,
    PROFILE_UPDATE_ATTEMPT_LIMIT, REGISTER_ATTEMPT_LIMIT, crypto::rate_limited,
    internal::record_audit,
};
use crate::features::auth::{
    model::{AuthUser, ChangePasswordRequest, LoginRequest, ProfileRequest, RegisterRequest},
    password,
};

impl AuthService {
    pub async fn register(
        &self,
        request: RegisterRequest,
        request_ip: IpAddr,
    ) -> Result<LoginOutcome, AppError> {
        request.validate()?;
        let username = request.username.trim().to_owned();
        let display_name = request.display_name.trim().to_owned();
        let request_ip_text = request_ip.to_string();
        if self.recent_auth_action_count("auth.register", &request_ip_text).await?
            >= REGISTER_ATTEMPT_LIMIT
        {
            return Err(rate_limited());
        }
        let password_hash = password::hash(request.password.clone())
            .await
            .map_err(|e| AppError::internal("hash registration password", e))?;
        let inserted=sqlx::query("INSERT INTO users(username,password_hash,display_name,user_type) VALUES($1,$2,$3,'INDIVIDUAL')")
            .bind(&username).bind(password_hash).bind(&display_name).execute(&self.database).await;
        match inserted {
            Ok(_) => {
                self.record_auth_action("auth.register", &request_ip_text, "success").await?;
                self.login(LoginRequest { username, password: request.password }, request_ip).await
            }
            Err(sqlx::Error::Database(error)) if error.constraint().is_some() => {
                self.record_auth_action_failure("auth.register", &request_ip_text).await?;
                Err(AppError::conflict("USERNAME_TAKEN", "Username is already registered"))
            }
            Err(error) => {
                self.record_auth_action_failure("auth.register", &request_ip_text).await?;
                Err(AppError::internal("create individual account", error))
            }
        }
    }
    pub async fn change_password(
        &self,
        session: &AuthenticatedSession,
        request: ChangePasswordRequest,
        request_ip: IpAddr,
    ) -> Result<AuthUser, AppError> {
        request.validate()?;
        let request_ip_text = request_ip.to_string();
        if self.recent_auth_action_count("PASSWORD_CHANGE_FAILED", &request_ip_text).await?
            >= PASSWORD_CHANGE_ATTEMPT_LIMIT
        {
            return Err(rate_limited());
        }
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
            self.record_auth_action_failure("PASSWORD_CHANGE_FAILED", &request_ip_text).await?;
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
            &request_ip_text,
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
    pub async fn update_profile(
        &self,
        session: &AuthenticatedSession,
        request: ProfileRequest,
        request_ip: IpAddr,
    ) -> Result<AuthUser, AppError> {
        let display_name = request.validate()?;
        let request_ip_text = request_ip.to_string();
        if self.recent_auth_action_count("PROFILE_UPDATED", &request_ip_text).await?
            >= PROFILE_UPDATE_ATTEMPT_LIMIT
        {
            return Err(rate_limited());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin profile update", error))?;
        let update = sqlx::query(
            "UPDATE users SET display_name=$1,updated_at=now() WHERE id=$2 AND enabled=true",
        )
        .bind(display_name)
        .bind(session.user.id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("update profile", error))?;
        if update.rows_affected() != 1 {
            return Err(AppError::unauthorized("ACCOUNT_DISABLED", "Account is disabled"));
        }
        record_audit(
            &mut transaction,
            Some(session.user.id),
            "PROFILE_UPDATED",
            &session.user.id.to_string(),
            &request_ip_text,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit profile update", error))?;
        self.load_user_by_id(session.user.id)
            .await?
            .ok_or_else(|| AppError::unauthorized("ACCOUNT_DISABLED", "Account is disabled"))?
            .auth_user()
    }
}
