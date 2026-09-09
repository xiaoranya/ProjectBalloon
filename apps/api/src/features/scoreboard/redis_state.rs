//! Redis-backed live scoreboard projection for running contests.
//!
//! PostgreSQL stays the single source of truth (submissions + judgements);
//! this module maintains a disposable, incrementally updated projection of
//! the scoreboard cells so the judgement path never re-scans a team/problem
//! history and the read path never aggregates in the database:
//!
//! - **Apply**: one atomic Lua script per completed judgement — O(1) instead
//!   of `rebuild_cell`'s full history scan.
//! - **Read**: `HGETALL` on the cells hash (or the frozen snapshot copy),
//!   aggregated in memory by `assemble`.
//! - **Repair**: any failed apply marks the cell dirty; the read path
//!   replays those cells from PostgreSQL (bounded to one cell's history).
//! - **Rebuild**: the whole projection can be discarded and replayed from
//!   submissions at any time (`rebuild_contest`).
//!
//! Keys (all per contest):
//! - `xcpc:sb:cells:{cid}`   hash `{team}:{problem}` → cell JSON
//! - `xcpc:sb:ver:{cid}`     monotonic change counter (cache key / ETag)
//! - `xcpc:sb:frozen:{cid}`  cells copy taken when the contest freezes
//! - `xcpc:sb:dirty:{cid}`   set of `{team}:{problem}` awaiting replay

use std::collections::HashMap;

use redis::cmd;
use sqlx::PgPool;
use time::OffsetDateTime;
use tracing::warn;

use crate::error::AppError;
use crate::features::redis::RedisHandle;
use crate::features::scoreboard::model::CellRow;

const CELLS_KEY_TEMPLATE: &str = "xcpc:sb:cells:{cid}";
const VERSION_KEY_TEMPLATE: &str = "xcpc:sb:ver:{cid}";
const FROZEN_KEY_TEMPLATE: &str = "xcpc:sb:frozen:{cid}";
const DIRTY_KEY_TEMPLATE: &str = "xcpc:sb:dirty:{cid}";

/// Verdicts that increment the ICPC wrong-attempt counter.
const PENALIZED: [&str; 5] = [
    "WRONG_ANSWER",
    "TIME_LIMIT_EXCEEDED",
    "MEMORY_LIMIT_EXCEEDED",
    "RUNTIME_ERROR",
    "OUTPUT_LIMIT_EXCEEDED",
];

/// One incremental judgement application. Runs atomically in Lua so two
/// workers finishing judgements for the same cell can never interleave a
/// read-modify-write.
const APPLY_SCRIPT: &str = r#"
local raw = redis.call('HGET', KEYS[1], ARGV[1])
local cell
if raw then
    cell = cjson.decode(raw)
else
    cell = {attempts = 0, wa = 0, solved = false, solved_at = 0, penalty = 0, score_milli = 0}
end
cell.attempts = cell.attempts + 1
local verdict = ARGV[2]
local t = tonumber(ARGV[3])
if ARGV[5] == 'ICPC' then
    if verdict == 'ACCEPTED' then
        if not cell.solved then
            cell.solved = true
            cell.solved_at = t
            cell.penalty = math.floor((t - tonumber(ARGV[4])) / 60000) + 20 * cell.wa
        end
    elseif verdict == 'WRONG_ANSWER' or verdict == 'TIME_LIMIT_EXCEEDED' or
           verdict == 'MEMORY_LIMIT_EXCEEDED' or verdict == 'RUNTIME_ERROR' or
           verdict == 'OUTPUT_LIMIT_EXCEEDED' then
        if (not cell.solved) or t < cell.solved_at then
            cell.wa = cell.wa + 1
        end
    end
else
    local score = tonumber(ARGV[7])
    if ARGV[6] == 'BEST' then
        if score > cell.score_milli then
            cell.score_milli = score
        end
    else
        cell.score_milli = score
    end
    if cell.score_milli >= tonumber(ARGV[8]) then
        if not cell.solved then
            cell.solved = true
            cell.solved_at = t
        end
    else
        cell.solved = false
        cell.solved_at = 0
    end
    cell.penalty = 0
end
redis.call('HSET', KEYS[1], ARGV[1], cjson.encode(cell))
redis.call('INCR', KEYS[2])
return 1
"#;

/// Overwrites one cell wholesale (used by replay paths) and bumps the version.
const REPLACE_SCRIPT: &str = r#"
if ARGV[2] == '' then
    redis.call('HDEL', KEYS[1], ARGV[1])
else
    redis.call('HSET', KEYS[1], ARGV[1], ARGV[2])
end
redis.call('INCR', KEYS[2])
return 1
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct CellState {
    #[serde(default)]
    attempts: i64,
    #[serde(default)]
    wa: i64,
    #[serde(default)]
    solved: bool,
    #[serde(default)]
    solved_at: i64,
    #[serde(default)]
    penalty: i64,
    #[serde(default)]
    score_milli: i64,
}

/// Everything the incremental apply needs to know about one completed
/// judgement (all values are already committed to PostgreSQL when this runs).
#[derive(Debug, Clone)]
pub struct ScoreEvent {
    pub contest_id: i64,
    pub team_id: i64,
    pub problem_id: i64,
    pub verdict: String,
    pub submitted_at_ms: i64,
    pub start_at_ms: i64,
    pub scoring_icpc: bool,
    pub aggregation_best: bool,
    pub score_milli: i64,
    pub max_score_milli: i64,
}

/// A consistent-ish read of the projection: the cells plus the version that
/// was observed with them (used as the whole-board cache key).
pub struct ProjectionSnapshot {
    pub(super) cells: Vec<CellRow>,
    pub(super) version: i64,
    pub(super) dirty: Vec<(i64, i64)>,
}

#[derive(Clone)]
pub struct ScoreboardProjection {
    redis: RedisHandle,
}

fn cells_key(contest_id: i64) -> String {
    CELLS_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

fn version_key(contest_id: i64) -> String {
    VERSION_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

fn frozen_key(contest_id: i64) -> String {
    FROZEN_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

fn dirty_key(contest_id: i64) -> String {
    DIRTY_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

fn cell_field(team_id: i64, problem_id: i64) -> String {
    format!("{team_id}:{problem_id}")
}

fn unix_ms(at: OffsetDateTime) -> i64 {
    (at.unix_timestamp_nanos() / 1_000_000) as i64
}

impl ScoreboardProjection {
    #[must_use]
    pub const fn new(redis: RedisHandle) -> Self {
        Self { redis }
    }

    /// Applies one completed judgement to the projection. Failure here is
    /// recoverable (the cell becomes dirty and is replayed on read), so the
    /// judge path treats errors as warnings, never as submission failures.
    pub async fn apply_judgement(&self, event: &ScoreEvent) -> Result<(), AppError> {
        let field = cell_field(event.team_id, event.problem_id);
        self.redis
            .query::<i64>(
                cmd("EVAL")
                    .arg(APPLY_SCRIPT)
                    .arg(2)
                    .arg(cells_key(event.contest_id))
                    .arg(version_key(event.contest_id))
                    .arg(&field)
                    .arg(&event.verdict)
                    .arg(event.submitted_at_ms)
                    .arg(event.start_at_ms)
                    .arg(if event.scoring_icpc { "ICPC" } else { "POINTS" })
                    .arg(if event.aggregation_best { "BEST" } else { "LAST" })
                    .arg(event.score_milli)
                    .arg(event.max_score_milli),
            )
            .await
            .map(|_| ())
    }

    /// Current projection version (the whole-board cache key). Zero means the
    /// projection was never populated for this contest.
    pub async fn version(&self, contest_id: i64) -> Result<i64, AppError> {
        Ok(self
            .redis
            .query::<Option<i64>>(cmd("GET").arg(version_key(contest_id)))
            .await?
            .unwrap_or(0))
    }

    /// Flags a cell whose incremental update could not be applied; the read
    /// path replays it from PostgreSQL.
    pub async fn mark_dirty(&self, contest_id: i64, team_id: i64, problem_id: i64) {
        let mut pipeline = redis::pipe();
        pipeline
            .sadd(dirty_key(contest_id), cell_field(team_id, problem_id))
            .ignore()
            .incr(version_key(contest_id), 1)
            .ignore();
        let _ = self.redis.query_pipeline::<()>(pipeline).await;
    }

    /// Reads the projection for one phase. `None` means the projection has no
    /// data for this phase (e.g. the frozen copy was never taken) and the
    /// caller must fall back to PostgreSQL.
    pub async fn read_cells(
        &self,
        contest_id: i64,
        phase: &str,
    ) -> Result<Option<ProjectionSnapshot>, AppError> {
        let source = if phase == "FROZEN" { frozen_key(contest_id) } else { cells_key(contest_id) };
        if phase == "FROZEN" {
            let exists: i64 = self.redis.query(cmd("EXISTS").arg(&source)).await?;
            if exists == 0 {
                return Ok(None);
            }
        }
        let raw_cells: Vec<(String, String)> =
            self.redis.query(cmd("HGETALL").arg(&source)).await?;
        let version: i64 = self
            .redis
            .query::<Option<i64>>(cmd("GET").arg(version_key(contest_id)))
            .await?
            .unwrap_or(0);
        let mut cells = Vec::with_capacity(raw_cells.len());
        for (field, payload) in raw_cells {
            let Some((team_id, problem_id)) = parse_field(&field) else { continue };
            match serde_json::from_str::<CellState>(&payload) {
                Ok(state) => cells.push(to_cell_row(team_id, problem_id, &state)),
                Err(error) => {
                    warn!(%error, %field, %contest_id, "ignoring malformed scoreboard cell")
                }
            }
        }
        let dirty: Vec<String> =
            self.redis.query(cmd("SMEMBERS").arg(dirty_key(contest_id))).await?;
        let dirty = dirty.iter().filter_map(|field| parse_field(field)).collect();
        Ok(Some(ProjectionSnapshot { cells, version, dirty }))
    }

    /// Recomputes one cell by replaying its (bounded) submission history from
    /// PostgreSQL — the authoritative path for rejudge and dirty repair.
    pub async fn recompute_cell(
        &self,
        database: &PgPool,
        contest_id: i64,
        team_id: i64,
        problem_id: i64,
    ) -> Result<(), sqlx::Error> {
        let meta = sqlx::query_as::<_, (Option<OffsetDateTime>, bool, bool)>(
            r#"
            SELECT c.start_at,
                   c.scoring_mode = 'ICPC',
                   c.score_aggregation = 'BEST'
            FROM contests c WHERE c.id = $1
            "#,
        )
        .bind(contest_id)
        .fetch_one(database)
        .await?;
        let max_score_milli = sqlx::query_scalar::<_, i64>(
            "SELECT coalesce(max_score_milli, 0) FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_one(database)
        .await
        .unwrap_or(0);
        let rows = sqlx::query_as::<_, (i64, String, i64, OffsetDateTime, String, Option<i64>)>(
            r#"
            SELECT s.id, j.verdict, coalesce(j.score_milli, 0), s.submitted_at, '', 0
            FROM submissions s
            JOIN judgements j ON j.submission_id = s.id
                AND j.active_marker IS TRUE AND j.completed_at IS NOT NULL
            WHERE s.contest_id = $1 AND s.team_id = $2 AND s.problem_id = $3
            ORDER BY s.submitted_at, s.id
            "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .bind(problem_id)
        .fetch_all(database)
        .await?;
        let Some(start_at) = meta.0 else { return Ok(()) };
        let replay: Vec<ReplayRow> = rows
            .into_iter()
            .map(|(submission_id, verdict, score_milli, submitted_at, _, _)| ReplayRow {
                submission_id,
                submitted_at_ms: unix_ms(submitted_at),
                verdict,
                score_milli,
            })
            .collect();
        let state = project_cell(unix_ms(start_at), meta.1, meta.2, max_score_milli, &replay);
        self.replace_cell(contest_id, team_id, problem_id, &state).await
    }

    /// Replays every submission of a contest into a fresh projection. Used to
    /// warm up on contest start and to recover from Redis data loss.
    pub async fn rebuild_contest(
        &self,
        database: &PgPool,
        contest_id: i64,
    ) -> Result<(), sqlx::Error> {
        let meta = sqlx::query_as::<_, (Option<OffsetDateTime>, bool, bool)>(
            r#"
            SELECT c.start_at, c.scoring_mode = 'ICPC', c.score_aggregation = 'BEST'
            FROM contests c WHERE c.id = $1
            "#,
        )
        .bind(contest_id)
        .fetch_one(database)
        .await?;
        let Some(start_at) = meta.0 else { return Ok(()) };
        let start_ms = unix_ms(start_at);
        let rows = sqlx::query_as::<_, (i64, i64, i64, OffsetDateTime, String, i64, i64)>(
            r#"
            SELECT s.team_id, s.problem_id, s.id, s.submitted_at, j.verdict,
                   coalesce(j.score_milli, 0), coalesce(cp.max_score_milli, 0)
            FROM submissions s
            JOIN judgements j ON j.submission_id = s.id
                AND j.active_marker IS TRUE AND j.completed_at IS NOT NULL
            JOIN contest_problems cp ON cp.contest_id = s.contest_id
                AND cp.problem_id = s.problem_id
            WHERE s.contest_id = $1
            ORDER BY s.submitted_at, s.id
            "#,
        )
        .bind(contest_id)
        .fetch_all(database)
        .await?;
        let mut grouped: HashMap<(i64, i64), (i64, Vec<ReplayRow>)> = HashMap::new();
        for (team_id, problem_id, submission_id, submitted_at, verdict, score_milli, max_score) in
            rows
        {
            let entry = grouped.entry((team_id, problem_id)).or_insert((max_score, Vec::new()));
            entry.1.push(ReplayRow {
                submission_id,
                submitted_at_ms: unix_ms(submitted_at),
                verdict,
                score_milli,
            });
        }
        let mut pipeline = redis::pipe();
        pipeline.del(cells_key(contest_id)).ignore();
        for ((team_id, problem_id), (max_score, replay)) in &grouped {
            let state = project_cell(start_ms, meta.1, meta.2, *max_score, replay);
            pipeline
                .hset(
                    cells_key(contest_id),
                    cell_field(*team_id, *problem_id),
                    serde_json::to_string(&state).map_err(|error| {
                        sqlx::Error::Protocol(format!("encode cell: {error:?}"))
                    })?,
                )
                .ignore();
        }
        pipeline.incr(version_key(contest_id), 1).ignore();
        pipeline.del(dirty_key(contest_id)).ignore();
        self.redis.query_pipeline::<()>(pipeline).await.map_err(|error| {
            sqlx::Error::Protocol(format!("rebuild contest projection: {error:?}"))
        })
    }

    /// Copies the live cells hash into the frozen snapshot when the contest
    /// freezes. Idempotent: existing copies are never overwritten.
    pub async fn freeze_snapshot(&self, contest_id: i64) -> Result<(), AppError> {
        let exists: i64 = self.redis.query(cmd("EXISTS").arg(frozen_key(contest_id))).await?;
        if exists == 0 {
            let copied: i64 = self
                .redis
                .query(cmd("COPY").arg(cells_key(contest_id)).arg(frozen_key(contest_id)))
                .await?;
            if copied == 0 {
                warn!(%contest_id, "no live cells to freeze; public board falls back to PostgreSQL");
            }
        }
        Ok(())
    }

    async fn replace_cell(
        &self,
        contest_id: i64,
        team_id: i64,
        problem_id: i64,
        state: &CellState,
    ) -> Result<(), sqlx::Error> {
        let payload = serde_json::to_string(state)
            .map_err(|error| sqlx::Error::Protocol(format!("encode cell: {error:?}")))?;
        self.redis
            .query_pipeline::<()>({
                let mut pipeline = redis::pipe();
                pipeline
                    .cmd("EVAL")
                    .arg(REPLACE_SCRIPT)
                    .arg(2)
                    .arg(cells_key(contest_id))
                    .arg(version_key(contest_id))
                    .arg(cell_field(team_id, problem_id))
                    .arg(&payload)
                    .ignore();
                pipeline.srem(dirty_key(contest_id), cell_field(team_id, problem_id)).ignore();
                pipeline
            })
            .await
            .map_err(|error| sqlx::Error::Protocol(format!("replace scoreboard cell: {error:?}")))
    }
}

struct ReplayRow {
    #[allow(dead_code)]
    submission_id: i64,
    submitted_at_ms: i64,
    verdict: String,
    score_milli: i64,
}

/// Pure in-memory replay of one cell's completed judgements — the same rules
/// as `APPLY_SCRIPT`, used by `recompute_cell`/`rebuild_contest` and unit
/// tested against the incremental path.
fn project_cell(
    start_at_ms: i64,
    scoring_icpc: bool,
    aggregation_best: bool,
    max_score_milli: i64,
    rows: &[ReplayRow],
) -> CellState {
    let mut cell = CellState::default();
    for row in rows {
        cell.attempts += 1;
        let t = row.submitted_at_ms;
        if scoring_icpc {
            if row.verdict == "ACCEPTED" {
                if !cell.solved {
                    cell.solved = true;
                    cell.solved_at = t;
                    cell.penalty = (t - start_at_ms).div_euclid(60_000) + 20 * cell.wa;
                }
            } else if PENALIZED.contains(&row.verdict.as_str())
                && (!cell.solved || t < cell.solved_at)
            {
                cell.wa += 1;
            }
        } else {
            if aggregation_best {
                if row.score_milli > cell.score_milli {
                    cell.score_milli = row.score_milli;
                }
            } else {
                cell.score_milli = row.score_milli;
            }
            if cell.score_milli >= max_score_milli {
                if !cell.solved {
                    cell.solved = true;
                    cell.solved_at = t;
                }
            } else {
                cell.solved = false;
                cell.solved_at = 0;
            }
            cell.penalty = 0;
        }
    }
    cell
}

fn to_cell_row(team_id: i64, problem_id: i64, state: &CellState) -> CellRow {
    CellRow {
        team_id,
        problem_id,
        wrong_attempts: i32::try_from(state.wa).unwrap_or(i32::MAX),
        solved: state.solved,
        solved_at: (state.solved_at > 0)
            .then(|| {
                OffsetDateTime::from_unix_timestamp_nanos(state.solved_at as i128 * 1_000_000).ok()
            })
            .flatten(),
        penalty_minutes: state.penalty,
        score_milli: i32::try_from(state.score_milli).unwrap_or(i32::MAX),
    }
}

fn parse_field(field: &str) -> Option<(i64, i64)> {
    let (team, problem) = field.split_once(':')?;
    Some((team.parse().ok()?, problem.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(at_offset_ms: i64, verdict: &str, score: i64) -> ReplayRow {
        ReplayRow {
            submission_id: 1,
            submitted_at_ms: 1_000_000 + at_offset_ms,
            verdict: verdict.to_owned(),
            score_milli: score,
        }
    }

    /// The incremental Lua rules and the batch replay must agree cell by cell;
    /// this test pins the replay implementation to the documented ICPC rules
    /// (the Lua side is exercised by the Redis integration tests).
    #[test]
    fn icpc_replay_matches_documented_rules() {
        let start = 1_000_000;
        let rows = [
            row(60_000, "WRONG_ANSWER", 0),
            row(120_000, "TIME_LIMIT_EXCEEDED", 0),
            row(1_800_000, "RUNTIME_ERROR", 0),
            row(3_600_000, "ACCEPTED", 0),
            row(3_700_000, "WRONG_ANSWER", 0), // after AC: ignored
        ];
        let cell = project_cell(start, true, false, 0, &rows);
        assert_eq!(cell.wa, 3);
        assert!(cell.solved);
        assert_eq!(cell.solved_at, start + 3_600_000);
        assert_eq!(
            cell.penalty,
            (3_600_000 / 60_000) + 20 * 3,
            "penalty = minutes until AC + 20 per prior rejection"
        );
    }

    #[test]
    fn points_replay_tracks_best_and_last() {
        let start = 1_000_000;
        let rows = [row(60_000, "PARTIAL", 4_000), row(120_000, "PARTIAL", 7_500)];
        let best = project_cell(start, false, true, 10_000, &rows);
        assert_eq!(best.score_milli, 7_500);
        assert!(!best.solved);

        let last = project_cell(start, false, false, 7_500, &rows);
        assert_eq!(last.score_milli, 7_500);
        assert!(last.solved, "LAST aggregation reaches max score");
        assert_eq!(last.penalty, 0);
    }
}
