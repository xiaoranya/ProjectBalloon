use sqlx::{Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;

use crate::features::auth::model::UserRow;
use crate::features::auth::service::{AuthService, USER_COLUMNS};


/// Sliding-window rate limiting lives exclusively in Redis: one sorted set per
/// (action, IP) whose members are unique attempt markers scored by attempt
/// time. The window is the same five-minute sliding window the previous
/// `audit_logs`-backed counters enforced.
const RATE_LIMIT_KEY_PREFIX: &str = "xcpc:ratelimit:v1:";
const RATE_LIMIT_WINDOW_MILLIS: i64 = 5 * 60 * 1000;

/// Atomically trims the window, checks the limit, and records the attempt.
/// Returns 1 when the attempt is within the limit (and was recorded), 0 when
/// the caller has exhausted the limit.
const RATE_LIMIT_SCRIPT: &str = r#"
redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', ARGV[1])
if redis.call('ZCARD', KEYS[1]) >= tonumber(ARGV[2]) then
    return 0
end
redis.call('ZADD', KEYS[1], ARGV[1], ARGV[3])
redis.call('PEXPIRE', KEYS[1], tonumber(ARGV[4]))
return 1
"#;

fn rate_limit_key(action: &str, request_ip: &str) -> String {
    format!("{RATE_LIMIT_KEY_PREFIX}{action}:{request_ip}")
}

fn now_millis() -> i64 {
    (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

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
        let redis = self.redis()?;
        let key = rate_limit_key("auth.login", request_ip);
        let now = now_millis();
        let mut pipeline = redis::pipe();
        pipeline
            .cmd("ZREMRANGEBYSCORE")
            .arg(&key)
            .arg("-inf")
            .arg(now - RATE_LIMIT_WINDOW_MILLIS)
            .ignore();
        pipeline.cmd("ZCARD").arg(&key);
        let ((), count): ((), i64) = redis.query_pipeline(pipeline).await?;
        Ok(count)
    }
    /// Records a failed login atomically against the Redis sliding window and
    /// mirrors the attempt into `audit_logs` (kept for the audit trail; it no
    /// longer participates in counting). Returns `false` once the limit is
    /// exhausted.
    pub(super) async fn record_failed_login(
        &self,
        username: &str,
        request_ip: &str,
        limit: i64,
    ) -> Result<bool, AppError> {
        let allowed = self.record_rate_limit_attempt("auth.login", request_ip, limit).await?;
        if allowed {
            self.record_auth_action("auth.login", &username.to_lowercase(), request_ip, "failed")
                .await?;
        }
        Ok(allowed)
    }
    /// Runs the atomic check-and-record script for `action`; the caller decides
    /// whether the attempt also lands in the audit trail.
    pub(super) async fn record_rate_limit_attempt(
        &self,
        action: &str,
        request_ip: &str,
        limit: i64,
    ) -> Result<bool, AppError> {
        let redis = self.redis()?;
        let now = now_millis();
        let member = format!("{now}:{}", Uuid::new_v4());
        let allowed: i32 = redis
            .query(redis::cmd("EVAL").arg(RATE_LIMIT_SCRIPT).arg(1)
                .arg(rate_limit_key(action, request_ip))
                .arg(now)
                .arg(limit)
                .arg(&member)
                .arg(RATE_LIMIT_WINDOW_MILLIS))
            .await?;
        Ok(allowed == 1)
    }
    pub(super) async fn recent_auth_action_count(
        &self,
        action: &str,
        request_ip: &str,
    ) -> Result<i64, AppError> {
        let redis = self.redis()?;
        let key = rate_limit_key(action, request_ip);
        let now = now_millis();
        let mut pipeline = redis::pipe();
        pipeline
            .cmd("ZREMRANGEBYSCORE")
            .arg(&key)
            .arg("-inf")
            .arg(now - RATE_LIMIT_WINDOW_MILLIS)
            .ignore();
        pipeline.cmd("ZCARD").arg(&key);
        let ((), count): ((), i64) = redis.query_pipeline(pipeline).await?;
        Ok(count)
    }
    pub(super) async fn record_auth_action_failure(
        &self,
        action: &str,
        request_ip: &str,
    ) -> Result<(), AppError> {
        self.record_auth_action(action, "", request_ip, "failed").await
    }
    pub(super) async fn record_auth_action(
        &self,
        action: &str,
        target_id: &str,
        request_ip: &str,
        result: &str,
    ) -> Result<(), AppError> {
        let redis = self.redis()?;
        let now = now_millis();
        let member = format!("{now}:{}", Uuid::new_v4());
        let mut pipeline = redis::pipe();
        pipeline
            .cmd("ZADD")
            .arg(rate_limit_key(action, request_ip))
            .arg(now)
            .arg(&member)
            .ignore();
        pipeline
            .cmd("PEXPIRE")
            .arg(rate_limit_key(action, request_ip))
            .arg(RATE_LIMIT_WINDOW_MILLIS)
            .ignore();
        redis.query_pipeline::<()>(pipeline).await?;
        sqlx::query(
            "INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES(NULL,$1,'user',$2,$3,$4)",
        )
        .bind(action)
        .bind(target_id)
        .bind(request_ip)
        .bind(result)
        .execute(&self.database)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("record authentication action failure", error))
    }
    pub(super) async fn delete_session(&self, token_hash: &str) -> Result<(), AppError> {
        super::store::delete_session(self.redis()?, token_hash).await
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
