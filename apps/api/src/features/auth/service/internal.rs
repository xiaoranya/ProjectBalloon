use sqlx::{Postgres, Transaction};

use crate::error::AppError;

use super::super::model::UserRow;
use super::{AuthService, USER_COLUMNS};

impl AuthService {
    pub(super) async fn load_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRow>, AppError> {
        let query = format!(
            r#"
            SELECT {USER_COLUMNS}
            FROM users u
            LEFT JOIN user_permissions up ON up.user_id = u.id
            LEFT JOIN permissions p ON p.id = up.permission_id
            WHERE u.username = $1
            GROUP BY u.id
            "#
        );
        sqlx::query_as::<_, UserRow>(sqlx::AssertSqlSafe(query))
            .bind(username)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load user by username", error))
    }
    pub(super) async fn load_user_by_id(&self, user_id: i64) -> Result<Option<UserRow>, AppError> {
        let query = format!(
            r#"
            SELECT {USER_COLUMNS}
            FROM users u
            LEFT JOIN user_permissions up ON up.user_id = u.id
            LEFT JOIN permissions p ON p.id = up.permission_id
            WHERE u.id = $1
            GROUP BY u.id
            "#
        );
        sqlx::query_as::<_, UserRow>(sqlx::AssertSqlSafe(query))
            .bind(user_id)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load user by ID", error))
    }
    pub(super) async fn failed_login_count(&self, request_ip: &str) -> Result<i64, AppError> {
        sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM audit_logs
            WHERE action = 'auth.login'
              AND result = 'failed'
              AND request_ip = $1
              AND created_at > now() - interval '5 minutes'
            "#,
        )
        .bind(request_ip)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check login rate limit", error))
    }
    pub(super) async fn record_failed_login(
        &self,
        username: &str,
        request_ip: &str,
        limit: i64,
    ) -> Result<bool, AppError> {
        sqlx::query_scalar(
            r#"
            WITH locked AS (
                SELECT pg_advisory_xact_lock(hashtext($1))
            ), attempts AS (
                SELECT count(*) AS count
                FROM audit_logs, locked
                WHERE action = 'auth.login'
                  AND result = 'failed'
                  AND request_ip = $1
                  AND created_at > now() - interval '5 minutes'
            ), inserted AS (
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, request_ip, result)
            SELECT NULL, 'auth.login', 'user', $2, $1, 'failed'
            FROM attempts
            WHERE count < $3
            RETURNING id
            )
            SELECT EXISTS(SELECT 1 FROM inserted)
            "#,
        )
        .bind(request_ip)
        .bind(username.to_lowercase())
        .bind(limit)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("record failed login", error))
    }
    pub(super) async fn recent_auth_action_count(
        &self,
        action: &str,
        request_ip: &str,
    ) -> Result<i64, AppError> {
        sqlx::query_scalar(
            "SELECT count(*) FROM audit_logs WHERE action=$1 AND request_ip=$2 AND created_at > now()-interval '5 minutes'",
        )
        .bind(action)
        .bind(request_ip)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check authentication action rate limit", error))
    }
    pub(super) async fn record_auth_action_failure(
        &self,
        action: &str,
        request_ip: &str,
    ) -> Result<(), AppError> {
        self.record_auth_action(action, request_ip, "failed").await
    }
    pub(super) async fn record_auth_action(
        &self,
        action: &str,
        request_ip: &str,
        result: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES(NULL,$1,'user','',$2,$3)",
        )
        .bind(action)
        .bind(request_ip)
        .bind(result)
        .execute(&self.database)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("record authentication action failure", error))
    }
    pub(super) async fn delete_session(&self, token_hash: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM auth_sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.database)
            .await
            .map(|_| ())
            .map_err(|error| AppError::internal("delete authentication session", error))
    }
}

pub(super) async fn record_audit(
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
