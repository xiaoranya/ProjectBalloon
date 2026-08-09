use sqlx::PgPool;
use thiserror::Error;

use crate::features::auth::hash_password;

const BOOTSTRAP_LOCK_ID: i64 = 0x0050_4242_4f4f_5453;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapAdmin {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("invalid bootstrap configuration: {0}")]
    Invalid(&'static str),
    #[error("administrator bootstrap is unavailable because the users table is not empty")]
    AlreadyInitialized,
    #[error("failed to hash the bootstrap password")]
    Password,
    #[error("bootstrap database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

impl BootstrapAdmin {
    pub fn new(
        username: String,
        display_name: String,
        password: String,
    ) -> Result<Self, BootstrapError> {
        let username = username.trim().to_ascii_lowercase();
        if !(3..=64).contains(&username.len())
            || !username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(BootstrapError::Invalid(
                "username must contain 3 to 64 ASCII letters, digits, dots, underscores, or hyphens",
            ));
        }

        let display_name = display_name.trim().to_owned();
        if display_name.is_empty() || display_name.chars().count() > 128 {
            return Err(BootstrapError::Invalid(
                "display name must contain between 1 and 128 characters",
            ));
        }
        if !(12..=128).contains(&password.chars().count()) {
            return Err(BootstrapError::Invalid(
                "password must contain between 12 and 128 characters",
            ));
        }

        Ok(Self { username, display_name, password })
    }
}

pub async fn bootstrap_super_admin(
    database: &PgPool,
    admin: BootstrapAdmin,
) -> Result<i64, BootstrapError> {
    let password_hash =
        hash_password(admin.password).await.map_err(|_| BootstrapError::Password)?;
    let mut transaction = database.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BOOTSTRAP_LOCK_ID)
        .execute(&mut *transaction)
        .await?;

    let user_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
        .fetch_one(&mut *transaction)
        .await?;
    if user_count != 0 {
        return Err(BootstrapError::AlreadyInitialized);
    }

    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO users
            (username, password_hash, display_name, user_type, enabled,
             password_reset_required)
        VALUES ($1, $2, $3, 'SUPER_ADMIN', true, true)
        RETURNING id
        "#,
    )
    .bind(admin.username)
    .bind(password_hash)
    .bind(admin.display_name)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES (NULL, 'SUPER_ADMIN_BOOTSTRAPPED', 'USER', $1, NULL, 'SUCCESS')
        "#,
    )
    .bind(user_id.to_string())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(user_id)
}

#[cfg(test)]
mod tests {
    use super::BootstrapAdmin;

    #[test]
    fn input_is_normalized() {
        let admin = BootstrapAdmin::new(
            " Admin.Root ".to_owned(),
            " Platform Administrator ".to_owned(),
            "a-long-bootstrap-password".to_owned(),
        )
        .expect("valid bootstrap input");

        assert_eq!(admin.username, "admin.root");
        assert_eq!(admin.display_name, "Platform Administrator");
    }

    #[test]
    fn weak_or_malformed_input_is_rejected() {
        assert!(
            BootstrapAdmin::new(
                "bad username".to_owned(),
                "Administrator".to_owned(),
                "a-long-bootstrap-password".to_owned(),
            )
            .is_err()
        );
        assert!(
            BootstrapAdmin::new(
                "admin".to_owned(),
                "Administrator".to_owned(),
                "too-short".to_owned(),
            )
            .is_err()
        );
    }
}
