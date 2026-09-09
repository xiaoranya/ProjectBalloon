//! Redis-backed realtime event outbox.
//!
//! The outbox replaces the previous PostgreSQL `realtime_outbox` table:
//!
//! - **Enqueue** writes a JSON entry to the `xcpc:realtime:outbox` stream and
//!   mirrors it into a per-contest replay index (a sorted set keyed by event
//!   time) that serves SSE `Last-Event-ID` resume, so PostgreSQL no longer
//!   participates in the realtime path at all.
//! - **Delivery** uses a Redis consumer group (see [`super::dispatcher`]):
//!   entries are acknowledged after confirmed fan-out, stuck entries are
//!   reclaimed after the lease window, and entries exceeding the attempt
//!   budget are dead-lettered into a counter for observability.
//! - **Replay** is answered from the per-contest replay index, which is
//!   trimmed to the replay window on every enqueue.
//!
//! Consistency note: unlike the PostgreSQL outbox, an enqueue is not atomic
//! with the surrounding business transaction. Realtime events are refresh
//! hints — PostgreSQL remains the authoritative data source and clients fall
//! back to polling — so a phantom event after a rollback or a lost event on a
//! crash between commit and enqueue only costs a spurious or missing hint.

use std::time::Duration;

use project_balloon_contracts::{RealtimeEvent, RealtimeScope};
use redis::cmd;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::features::redis::RedisHandle;

pub(crate) const OUTBOX_STREAM_KEY: &str = "xcpc:realtime:outbox";
const DEAD_LETTER_KEY: &str = "xcpc:realtime:outbox:dead";
const REPLAY_KEY_PREFIX: &str = "xcpc:realtime:replay:";
const ANCHOR_KEY_PREFIX: &str = "xcpc:realtime:anchor:";
const CONSUMER_GROUP: &str = "dispatchers";
/// Matches the SSE replay window (`handlers::REPLAY_WINDOW`).
const REPLAY_WINDOW_MILLIS: i64 = 5 * 60 * 1000;
/// Matches the SSE replay cap (`handlers::REPLAY_MAX_EVENTS`).
const REPLAY_MAX_EVENTS: usize = 100;
/// Hard cap of replay index members per contest; the window trims the rest.
const REPLAY_INDEX_CAP: isize = 2_000;
/// Current realtime event schema version stamped onto every outbox entry.
const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub event_id: Uuid,
    pub contest_id: i64,
    pub event_type: String,
    pub schema_version: u16,
    pub scope: String,
    pub team_id: Option<i64>,
    pub occurred_at_millis: i64,
    pub payload: serde_json::Value,
}

impl OutboxEntry {
    #[must_use]
    pub fn new(
        contest_id: i64,
        event_type: &str,
        scope: RealtimeScope,
        team_id: Option<i64>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            contest_id,
            event_type: event_type.to_owned(),
            schema_version: SCHEMA_VERSION,
            scope: scope.as_str().to_owned(),
            team_id,
            occurred_at_millis: now_millis(),
            payload,
        }
    }

    fn replay_score(&self) -> f64 {
        // Microsecond precision keeps distinct events ordered while staying
        // exactly representable as an f64 score (< 2^53).
        self.occurred_at_millis as f64 * 1_000.0
    }

    fn into_event(self, contest_id: i64) -> Option<RealtimeEvent> {
        let scope = super::dispatcher::parse_scope(&self.scope)?;
        Some(RealtimeEvent {
            id: self.event_id,
            version: self.schema_version,
            event_type: self.event_type,
            scope,
            contest_id,
            occurred_at: time::OffsetDateTime::from_unix_timestamp_nanos(
                i128::from(self.occurred_at_millis) * 1_000_000,
            )
            .ok()?,
            payload: self.payload,
        })
    }
}

fn now_millis() -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

fn replay_key(contest_id: i64) -> String {
    format!("{REPLAY_KEY_PREFIX}{contest_id}")
}

fn anchor_key(event_id: Uuid) -> String {
    format!("{ANCHOR_KEY_PREFIX}{event_id}")
}

#[derive(Clone)]
pub struct RealtimeOutbox {
    redis: RedisHandle,
}

impl RealtimeOutbox {
    #[must_use]
    pub fn new(redis: RedisHandle) -> Self {
        Self { redis }
    }

    /// Enqueues an event for delivery and indexes it for SSE replay. Errors
    /// are surfaced to the caller; enqueue sites treat this as best-effort so
    /// a Redis blip cannot fail the surrounding business operation.
    pub async fn enqueue(&self, entry: &OutboxEntry) -> Result<(), AppError> {
        let payload = serde_json::to_string(entry)
            .map_err(|error| AppError::internal("encode realtime outbox entry", error))?;
        let window_floor = now_millis() - REPLAY_WINDOW_MILLIS;
        let mut pipeline = redis::pipe();
        pipeline.cmd("XADD").arg(OUTBOX_STREAM_KEY).arg("*").arg("entry").arg(&payload).ignore();
        pipeline
            .cmd("ZADD")
            .arg(replay_key(entry.contest_id))
            .arg(entry.replay_score())
            .arg(&payload)
            .ignore();
        pipeline
            .cmd("SET")
            .arg(anchor_key(entry.event_id))
            .arg(format!("{}\u{1f}{}", entry.contest_id, entry.replay_score()))
            .arg("EX")
            .arg(REPLAY_WINDOW_MILLIS / 1000)
            .ignore();
        pipeline
            .cmd("ZREMRANGEBYSCORE")
            .arg(replay_key(entry.contest_id))
            .arg("-inf")
            .arg(format!("({window_floor}"))
            .ignore();
        pipeline
            .cmd("ZREMRANGEBYRANK")
            .arg(replay_key(entry.contest_id))
            .arg(0)
            .arg(-(REPLAY_INDEX_CAP + 1))
            .ignore();
        pipeline
            .cmd("EXPIRE")
            .arg(replay_key(entry.contest_id))
            .arg(REPLAY_WINDOW_MILLIS / 1000 * 3)
            .ignore();
        self.redis.query_pipeline::<()>(pipeline).await
    }

    /// Creates the consumer group if it does not exist yet (`$` is avoided on
    /// purpose: the group starts at `0` so events enqueued during startup are
    /// still delivered).
    pub(crate) async fn ensure_consumer_group(&self) -> Result<(), AppError> {
        match self
            .redis
            .query_value(
                cmd("XGROUP")
                    .arg("CREATE")
                    .arg(OUTBOX_STREAM_KEY)
                    .arg(CONSUMER_GROUP)
                    .arg("0")
                    .arg("MKSTREAM"),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) => {
                if format!("{error:?}").contains("BUSYGROUP") {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }

    /// Reads up to `count` never-delivered entries for this consumer group.
    pub(crate) async fn read_new(
        &self,
        consumer: &str,
        count: usize,
    ) -> Result<Vec<(String, OutboxEntry)>, AppError> {
        let mut read = cmd("XREADGROUP");
        read.arg("GROUP")
            .arg(CONSUMER_GROUP)
            .arg(consumer)
            .arg("COUNT")
            .arg(count)
            .arg("STREAMS")
            .arg(OUTBOX_STREAM_KEY)
            .arg(">");
        let reply = self.redis.query_value(&mut read).await?;
        let mut parsed = Vec::new();
        for (stream_id, fields) in Self::parse_xread(reply) {
            let Some(payload) = fields.get("entry") else { continue };
            match serde_json::from_str::<OutboxEntry>(payload) {
                Ok(entry) => parsed.push((stream_id, entry)),
                Err(error) => {
                    tracing::warn!(%error, %stream_id, "ignoring malformed outbox entry")
                }
            }
        }
        Ok(parsed)
    }

    /// Reclaims entries whose delivery lease expired (idle longer than
    /// `lease`), returning them with their delivery count so the caller can
    /// dead-letter exhausted entries.
    pub(crate) async fn reclaim_expired(
        &self,
        consumer: &str,
        lease: Duration,
        count: usize,
    ) -> Result<Vec<(String, OutboxEntry, i64)>, AppError> {
        let min_idle = i64::try_from(lease.as_millis()).unwrap_or(i64::MAX);
        let pending: Vec<(String, String, i64, i64)> = self
            .redis
            .query(
                cmd("XPENDING")
                    .arg(OUTBOX_STREAM_KEY)
                    .arg(CONSUMER_GROUP)
                    .arg("IDLE")
                    .arg(min_idle)
                    .arg("-")
                    .arg("+")
                    .arg(count),
            )
            .await?;
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<&str> = pending.iter().map(|(id, _, _, _)| id.as_str()).collect();
        let mut claim = cmd("XCLAIM");
        claim.arg(OUTBOX_STREAM_KEY).arg(CONSUMER_GROUP).arg(consumer).arg(min_idle);
        for id in &ids {
            claim.arg(id);
        }
        let claim_reply = self.redis.query_value(&mut claim).await?;
        let entries = Self::parse_xread(claim_reply);
        let mut reclaimed = Vec::with_capacity(entries.len());
        for (stream_id, fields) in entries {
            let Some(payload) = fields.get("entry") else { continue };
            let Ok(entry) = serde_json::from_str::<OutboxEntry>(payload) else {
                tracing::warn!(%stream_id, "ignoring malformed outbox entry");
                continue;
            };
            let Some((_, _, _, delivery_count)) =
                pending.iter().find(|(id, _, _, _)| *id == stream_id)
            else {
                continue;
            };
            reclaimed.push((stream_id, entry, *delivery_count));
        }
        Ok(reclaimed)
    }

    /// Acknowledges a successfully delivered (or dead-lettered) entry.
    pub(crate) async fn ack(&self, stream_id: &str) -> Result<(), AppError> {
        self.redis
            .query::<()>(cmd("XACK").arg(OUTBOX_STREAM_KEY).arg(CONSUMER_GROUP).arg(stream_id))
            .await
    }

    /// Records a dead-lettered entry for observability.
    pub(crate) async fn record_dead_letter(&self) -> Result<(), AppError> {
        self.redis.query::<()>(cmd("INCR").arg(DEAD_LETTER_KEY)).await
    }

    /// Health snapshot: entries awaiting confirmed delivery (pending in the
    /// consumer group) and dead-lettered entries so far.
    pub async fn health(&self) -> Result<(i64, i64), AppError> {
        let pending: Vec<redis::Value> =
            self.redis.query(cmd("XPENDING").arg(OUTBOX_STREAM_KEY).arg(CONSUMER_GROUP)).await?;
        let pending_count = pending
            .first()
            .and_then(|value| redis::FromRedisValue::from_redis_value(value.clone()).ok())
            .unwrap_or(0);
        let failed: Option<i64> = self.redis.query(cmd("GET").arg(DEAD_LETTER_KEY)).await?;
        Ok((pending_count, failed.unwrap_or(0)))
    }

    /// Replays in-scope events published after `last_event_id`, bounded by the
    /// replay window and a hard cap, in publication order. An unknown anchor
    /// or an expired one simply yields no replay — the client's poll refresh
    /// covers gaps.
    pub async fn load_replay(
        &self,
        contest_id: i64,
        scope: RealtimeScope,
        team_id: Option<i64>,
        last_event_id: Uuid,
    ) -> Result<Vec<RealtimeEvent>, AppError> {
        let anchor: Option<String> =
            self.redis.query(cmd("GET").arg(anchor_key(last_event_id))).await?;
        let Some(anchor) = anchor else { return Ok(Vec::new()) };
        let Some((anchor_contest, anchor_score)) = anchor.split_once('\u{1f}') else {
            return Ok(Vec::new());
        };
        if anchor_contest != contest_id.to_string() {
            return Ok(Vec::new());
        }
        let Ok(anchor_score) = anchor_score.parse::<f64>() else {
            return Ok(Vec::new());
        };
        let members: Vec<String> = self
            .redis
            .query(
                cmd("ZRANGEBYSCORE")
                    .arg(replay_key(contest_id))
                    .arg(format!("({anchor_score}"))
                    .arg("+inf")
                    .arg("LIMIT")
                    .arg(0)
                    .arg(REPLAY_MAX_EVENTS),
            )
            .await?;
        let mut events = Vec::new();
        for member in members {
            let Ok(entry) = serde_json::from_str::<OutboxEntry>(&member) else { continue };
            if entry.scope != scope.as_str() || entry.team_id != team_id {
                continue;
            }
            if let Some(event) = entry.into_event(contest_id) {
                events.push(event);
                if events.len() >= REPLAY_MAX_EVENTS {
                    break;
                }
            }
        }
        // The index is score-ordered ascending, which is publication order.
        Ok(events)
    }

    /// Parses a raw `XREAD`/`XCLAIM` reply: `[stream, [[id, [f, v, ...]], ...]]`
    /// into `(stream_id, field_map)` pairs. Null replies (no data) yield an
    /// empty vector.
    fn parse_xread(
        reply: redis::Value,
    ) -> Vec<(String, std::collections::HashMap<String, String>)> {
        use redis::Value::{Array, BulkString, Nil};
        let mut parsed = Vec::new();
        let streams = match reply {
            Nil => return parsed,
            Array(streams) => streams,
            _ => return parsed,
        };
        for stream in streams {
            let entries = match stream {
                Array(pair) if pair.len() == 2 => match pair.into_iter().nth(1) {
                    Some(Array(entries)) => entries,
                    _ => continue,
                },
                _ => continue,
            };
            for entry in entries {
                let (id, flat) = match entry {
                    Array(pair) if pair.len() == 2 => match (pair.first(), pair.get(1)) {
                        (Some(BulkString(id)), Some(Array(flat))) => {
                            (String::from_utf8_lossy(id).into_owned(), flat.clone())
                        }
                        _ => continue,
                    },
                    _ => continue,
                };
                let mut fields = std::collections::HashMap::new();
                for chunk in flat.chunks(2) {
                    if let [BulkString(key), BulkString(value)] = chunk {
                        fields.insert(
                            String::from_utf8_lossy(key).into_owned(),
                            String::from_utf8_lossy(value).into_owned(),
                        );
                    }
                }
                parsed.push((id, fields));
            }
        }
        parsed
    }
}

/// Shared enqueue entry point for the feature services. `None` (a process
/// started without Redis) degrades to a logged no-op: realtime events are
/// refresh hints and PostgreSQL remains the authoritative source.
pub(crate) async fn enqueue_optional(
    outbox: Option<&RealtimeOutbox>,
    contest_id: i64,
    event_type: &str,
    scope: &str,
    team_id: Option<i64>,
    payload: serde_json::Value,
) -> Result<(), AppError> {
    let Some(outbox) = outbox else {
        tracing::warn!(contest_id, event_type, "realtime outbox unavailable; dropping event");
        return Ok(());
    };
    let scope = super::dispatcher::parse_scope(scope)
        .ok_or_else(|| AppError::internal_message("invalid realtime scope", scope))?;
    outbox.enqueue(&OutboxEntry::new(contest_id, event_type, scope, team_id, payload)).await
}
