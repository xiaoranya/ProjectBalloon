use std::net::IpAddr;

use crate::error::AppError;
use crate::features::auth::{hash_password, model::AuthUser};

use super::TeamService;
use super::helpers::{record_audit, require_manage_team};

impl TeamService {
    pub async fn reset_password(
        &self,
        team_id: i64,
        new_password: String,
        require_password_reset: bool,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        let hash = hash_password(new_password)
            .await
            .map_err(|error| AppError::internal("hash team password", error))?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin team password reset", error))?;
        require_manage_team(&mut transaction, team_id, actor).await?;
        let user_id = sqlx::query_scalar::<_, i64>(
            "SELECT user_id FROM team_accounts WHERE team_id = $1 FOR UPDATE",
        )
        .bind(team_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock team account", error))?
        .ok_or_else(|| {
            AppError::not_found("TEAM_ACCOUNT_NOT_FOUND", "Team account was not found")
        })?;
        sqlx::query(
            "UPDATE users SET password_hash = $1, password_reset_required = $2, updated_at = now() WHERE id = $3",
        )
        .bind(hash)
        .bind(require_password_reset)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("reset team password", error))?;
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("revoke team sessions", error))?;
        record_audit(
            &mut transaction,
            actor.id,
            "TEAM_PASSWORD_RESET",
            "user",
            &user_id.to_string(),
            request_ip,
            "success",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit team password reset", error))
    }
}
