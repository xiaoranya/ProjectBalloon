//! Redis-backed login session storage.
//!
//! Sessions live exclusively in Redis: the canonical token hash (SHA-256 of
//! the raw session token) keys a JSON record whose TTL matches the session
//! TTL, so expiry is enforced by Redis itself. Two auxiliary indexes support
//! the revocation flows the previous PostgreSQL table answered with SQL:
//!
//! - `xcpc:session:v1:user:{user_id}` — set of token hashes owned by a user,
//!   used to revoke every other session on password change.
//! - `xcpc:session:v1:ws:{binding_id}` — token hash bound to a workstation,
//!   used to replace a workstation's previous session on re-login.

use redis::cmd;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::AppError;
use crate::features::redis::RedisHandle;

const SESSION_KEY_PREFIX: &str = "xcpc:session:v1:";
const USER_INDEX_PREFIX: &str = "xcpc:session:v1:user:";
const WORKSTATION_INDEX_PREFIX: &str = "xcpc:session:v1:ws:";

/// Minimum remaining lifetime (seconds) for a workstation session whose grant
/// is about to expire; shorter grants are clamped to this floor.
const MIN_SESSION_TTL_SECONDS: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SessionRecord {
    pub user_id: i64,
    pub access_fingerprint: String,
    pub created_at_millis: i64,
    pub last_seen_millis: i64,
    pub workstation_binding_id: Option<i64>,
    pub bound_ip: Option<String>,
}

fn session_key(token_hash: &str) -> String {
    format!("{SESSION_KEY_PREFIX}{token_hash}")
}

fn user_index_key(user_id: i64) -> String {
    format!("{USER_INDEX_PREFIX}{user_id}")
}

fn workstation_index_key(binding_id: i64) -> String {
    format!("{WORKSTATION_INDEX_PREFIX}{binding_id}")
}

/// Creates a normal (browser) session: the record and its user index entry
/// share the session TTL, so an expired session disappears together with its
/// index membership.
pub(super) async fn create_session(
    redis: &RedisHandle,
    token_hash: &str,
    record: &SessionRecord,
    ttl_seconds: u64,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(record)
        .map_err(|error| AppError::internal("encode session record", error))?;
    let mut pipeline = redis::pipe();
    pipeline
        .set_ex(session_key(token_hash), payload, ttl_seconds.max(1))
        .ignore()
        .sadd(user_index_key(record.user_id), token_hash)
        .ignore()
        .expire(user_index_key(record.user_id), (ttl_seconds.max(1)) as i64)
        .ignore();
    redis.query_pipeline::<()>(pipeline).await
}

/// Creates or replaces a workstation-bound session. Any session previously
/// bound to the same workstation binding is removed first, mirroring the
/// `DELETE FROM auth_sessions WHERE workstation_binding_id = $1` behavior.
pub(super) async fn create_workstation_session(
    redis: &RedisHandle,
    token_hash: &str,
    record: &SessionRecord,
    ttl_seconds: u64,
) -> Result<(), AppError> {
    let ttl_seconds = ttl_seconds.max(MIN_SESSION_TTL_SECONDS);
    if let Some(previous) = lookup_workstation_session(redis, record.workstation_binding_id).await?
    {
        delete_session(redis, &previous).await?;
    }
    let payload = serde_json::to_string(record)
        .map_err(|error| AppError::internal("encode workstation session record", error))?;
    let user_id = record.user_id;
    let binding_id = record.workstation_binding_id.expect("workstation record carries a binding");
    let mut pipeline = redis::pipe();
    pipeline
        .set_ex(session_key(token_hash), payload, ttl_seconds)
        .ignore()
        .set_ex(workstation_index_key(binding_id), token_hash, ttl_seconds)
        .ignore()
        .sadd(user_index_key(user_id), token_hash)
        .ignore()
        .expire(user_index_key(user_id), ttl_seconds as i64)
        .ignore();
    redis.query_pipeline::<()>(pipeline).await
}

/// Loads a live session record, or `None` when the token is unknown or the
/// session expired (Redis TTL removed the key).
pub(super) async fn load_session(
    redis: &RedisHandle,
    token_hash: &str,
) -> Result<Option<SessionRecord>, AppError> {
    let payload: Option<String> = redis.query(cmd("GET").arg(session_key(token_hash))).await?;
    let Some(payload) = payload else { return Ok(None) };
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|error| AppError::internal("decode session record", error))
}

/// Refreshes `last_seen_millis` (throttled by the caller to once per five
/// minutes) while preserving the remaining TTL via `KEEPTTL`.
pub(super) async fn touch_session(
    redis: &RedisHandle,
    token_hash: &str,
    record: &SessionRecord,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(record)
        .map_err(|error| AppError::internal("encode session record", error))?;
    redis.query::<()>(cmd("SET").arg(session_key(token_hash)).arg(payload).arg("KEEPTTL")).await
}

/// Deletes a session and both of its index entries. Returns silently when the
/// session is already gone.
pub(super) async fn delete_session(redis: &RedisHandle, token_hash: &str) -> Result<(), AppError> {
    let record = load_session(redis, token_hash).await?;
    let mut pipeline = redis::pipe();
    pipeline.del(session_key(token_hash)).ignore();
    if let Some(record) = &record {
        pipeline.srem(user_index_key(record.user_id), token_hash).ignore();
        if let Some(binding_id) = record.workstation_binding_id {
            pipeline.del(workstation_index_key(binding_id)).ignore();
        }
    }
    redis.query_pipeline::<()>(pipeline).await
}

/// Revokes every session of `user_id` except `keep_token_hash`, mirroring the
/// password-change flow. Workstation index entries of revoked sessions are
/// removed as well.
pub(super) async fn revoke_user_sessions(
    redis: &RedisHandle,
    user_id: i64,
    keep_token_hash: &str,
) -> Result<(), AppError> {
    let index_key = user_index_key(user_id);
    let members: Vec<String> = redis.query(cmd("SMEMBERS").arg(&index_key)).await?;
    let mut pipeline = redis::pipe();
    for member in members {
        if member == keep_token_hash {
            continue;
        }
        if let Some(record) = load_session(redis, &member).await?
            && let Some(binding_id) = record.workstation_binding_id
        {
            pipeline.del(workstation_index_key(binding_id));
        }
        pipeline.del(session_key(&member)).ignore();
        pipeline.srem(&index_key, &member).ignore();
    }
    redis.query_pipeline::<()>(pipeline).await
}

async fn lookup_workstation_session(
    redis: &RedisHandle,
    binding_id: Option<i64>,
) -> Result<Option<String>, AppError> {
    let Some(binding_id) = binding_id else { return Ok(None) };
    redis.query(cmd("GET").arg(workstation_index_key(binding_id))).await
}

/// Best-effort: removes a workstation index key whose session was already
/// replaced elsewhere.
#[allow(dead_code)]
pub(super) async fn clear_workstation_index(
    redis: &RedisHandle,
    binding_id: i64,
) -> Result<(), AppError> {
    redis.query::<()>(cmd("DEL").arg(workstation_index_key(binding_id))).await
}

/// Clamps a grant expiry to the configured session TTL and converts it to a
/// positive TTL in seconds.
pub(super) fn workstation_ttl_seconds(expires_at: OffsetDateTime, session_ttl_seconds: u64) -> u64 {
    let now = OffsetDateTime::now_utc();
    let capped = if expires_at < now { now } else { expires_at };
    let remaining = capped - now;
    let remaining_seconds = remaining.whole_seconds().max(0) as u64;
    remaining_seconds.min(session_ttl_seconds).max(MIN_SESSION_TTL_SECONDS)
}

/// Milliseconds since the Unix epoch for session bookkeeping timestamps.
pub(super) fn timestamp_millis(at: OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}
