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
use tracing::{error, warn};

use crate::error::AppError;
use crate::features::redis::RedisHandle;
use crate::features::scoreboard::model::CellRow;

const CELLS_KEY_TEMPLATE: &str = "xcpc:sb:cells:{cid}";
const VERSION_KEY_TEMPLATE: &str = "xcpc:sb:ver:{cid}";
const FROZEN_KEY_TEMPLATE: &str = "xcpc:sb:frozen:{cid}";
/// The roster as it looked at the freeze milestone. Without it a rename, a
/// group move or a deleted team after freeze rewrote the public frozen board.
const FROZEN_ROSTER_KEY_TEMPLATE: &str = "xcpc:sb:frozenroster:{cid}";
const DIRTY_KEY_TEMPLATE: &str = "xcpc:sb:dirty:{cid}";
/// Set when an incremental update could not be recorded, so the projection can
/// no longer be trusted and must be rebuilt before it is read.
const UNTRUSTED_KEY_TEMPLATE: &str = "xcpc:sb:untrusted:{cid}";
/// The trust flag only has to survive until the next read rebuilds the board.
const UNTRUSTED_TTL_SECONDS: i64 = 3_600;

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
    cell = {attempts = 0, wa = 0, solved = false, solved_at = 0, penalty = 0,
            score_milli = 0, last_at = 0}
end
-- Cells written before `last_at` existed decode without the field.
local last_at = cell.last_at or 0
cell.attempts = cell.attempts + 1
local verdict = ARGV[2]
local t = tonumber(ARGV[3])
local start_at = tonumber(ARGV[4])
if ARGV[5] == 'ICPC' then
    if verdict == 'ACCEPTED' then
        if not cell.solved then
            cell.solved = true
            cell.solved_at = t
            cell.penalty = math.floor((t - start_at) / 60000) + 20 * cell.wa
        elseif t < cell.solved_at then
            -- Judgements finish out of submission order, so an earlier
            -- accepted submission can arrive after a later one. `wa` may then
            -- contain rejections submitted after `t`, and the penalty cannot
            -- be derived incrementally: flag the cell so the read path
            -- replays it from PostgreSQL instead of showing a wrong penalty.
            redis.call('SADD', KEYS[3], ARGV[1])
        end
    elseif verdict == 'WRONG_ANSWER' or verdict == 'TIME_LIMIT_EXCEEDED' or
           verdict == 'MEMORY_LIMIT_EXCEEDED' or verdict == 'RUNTIME_ERROR' or
           verdict == 'OUTPUT_LIMIT_EXCEEDED' then
        if (not cell.solved) or t < cell.solved_at then
            cell.wa = cell.wa + 1
            if cell.solved then
                -- A rejection submitted before the accepted one: it adds the
                -- 20 minute penalty even though it finished later.
                cell.penalty = math.floor((cell.solved_at - start_at) / 60000) + 20 * cell.wa
            end
        end
    end
else
    local score = tonumber(ARGV[7])
    -- Only the newest submission may revoke a solve or overwrite a LAST score.
    local newest = t >= last_at
    if ARGV[6] == 'BEST' then
        if score > cell.score_milli then
            cell.score_milli = score
        end
    elseif newest then
        cell.score_milli = score
    end
    if cell.score_milli >= tonumber(ARGV[8]) then
        if not cell.solved then
            cell.solved = true
            cell.solved_at = t
        elseif t < cell.solved_at then
            -- Earliest full-score submission wins regardless of finish order.
            cell.solved_at = t
        end
    elseif newest then
        cell.solved = false
        cell.solved_at = 0
    end
    if newest then
        cell.last_at = t
    end
    cell.penalty = 0
    -- POINTS boards show every attempt, matching the PostgreSQL rebuild
    -- (`rebuild_points_cell`) and the submission-based fallback. Without this
    -- the live OI board reported zero attempts while a rebuilt one reported all
    -- of them.
    cell.wa = cell.attempts
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
    /// Submission time of the newest applied judgement (POINTS modes only);
    /// keeps `LAST` aggregation and solve revocation order-independent.
    #[serde(default)]
    last_at: i64,
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

fn frozen_roster_key(contest_id: i64) -> String {
    FROZEN_ROSTER_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

/// The roster fields the frozen board publishes, captured at freeze time.
#[derive(serde::Serialize, serde::Deserialize)]
struct FrozenRosterEntry {
    team_name: String,
    school: Option<String>,
    participation_type: String,
    group_name: Option<String>,
    team_star: bool,
}

fn dirty_key(contest_id: i64) -> String {
    DIRTY_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
}

fn untrusted_key(contest_id: i64) -> String {
    UNTRUSTED_KEY_TEMPLATE.replace("{cid}", &contest_id.to_string())
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
                    .arg(3)
                    .arg(cells_key(event.contest_id))
                    .arg(version_key(event.contest_id))
                    .arg(dirty_key(event.contest_id))
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
    ///
    /// A failure here would leave the stale cell in place *without* the dirty
    /// marker, so the board would keep serving it forever. Instead of swallowing
    /// the error, flag the whole projection untrusted: the next read rebuilds
    /// from PostgreSQL rather than trusting a board it can no longer repair.
    pub async fn mark_dirty(&self, contest_id: i64, team_id: i64, problem_id: i64) {
        let mut pipeline = redis::pipe();
        pipeline
            .sadd(dirty_key(contest_id), cell_field(team_id, problem_id))
            .ignore()
            .incr(version_key(contest_id), 1)
            .ignore();
        if let Err(error) = self.redis.query_pipeline::<()>(pipeline).await {
            error!(
                ?error,
                %contest_id,
                team_id,
                problem_id,
                "failed to flag a scoreboard cell dirty; marking the projection untrusted"
            );
            if let Err(error) = self.flag_untrusted(contest_id).await {
                error!(?error, %contest_id, "failed to flag the scoreboard projection untrusted");
            }
        }
    }

    /// Marks the whole projection for `contest_id` as needing a rebuild.
    async fn flag_untrusted(&self, contest_id: i64) -> Result<(), AppError> {
        self.redis
            .query::<()>(
                cmd("SET")
                    .arg(untrusted_key(contest_id))
                    .arg("1")
                    .arg("EX")
                    .arg(UNTRUSTED_TTL_SECONDS),
            )
            .await
    }

    /// Best-effort variant used when a rebuild failed: the caller has already
    /// logged the rebuild error, so a failure here is only worth a log line.
    pub(crate) async fn flag_untrusted_after_failed_rebuild(&self, contest_id: i64) {
        if let Err(error) = self.flag_untrusted(contest_id).await {
            error!(?error, %contest_id, "failed to flag the scoreboard projection untrusted");
        }
    }

    /// True when an earlier incremental update could not be recorded, so the
    /// projection must be rebuilt before it is read.
    pub async fn is_untrusted(&self, contest_id: i64) -> bool {
        match self.redis.query::<Option<String>>(cmd("GET").arg(untrusted_key(contest_id))).await {
            Ok(flag) => flag.is_some(),
            Err(error) => {
                // A Redis read failure means the caller cannot trust *any* of
                // this projection; the caller already treats that as a rebuild.
                warn!(?error, %contest_id, "cannot read the scoreboard trust flag");
                false
            }
        }
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

    /// The roster as captured at the freeze milestone, or `None` when the
    /// snapshot predates roster capture.
    pub(crate) async fn read_frozen_roster(
        &self,
        contest_id: i64,
    ) -> Result<Option<Vec<crate::features::scoreboard::model::RosterRow>>, AppError> {
        let key = frozen_roster_key(contest_id);
        let exists: i64 = self.redis.query(cmd("EXISTS").arg(&key)).await?;
        if exists == 0 {
            return Ok(None);
        }
        let raw: Vec<(String, String)> = self.redis.query(cmd("HGETALL").arg(&key)).await?;
        let mut roster = Vec::with_capacity(raw.len());
        for (field, payload) in raw {
            let Ok(team_id) = field.parse::<i64>() else { continue };
            match serde_json::from_str::<FrozenRosterEntry>(&payload) {
                Ok(entry) => roster.push(crate::features::scoreboard::model::RosterRow {
                    team_id,
                    team_name: entry.team_name,
                    school: entry.school,
                    participation_type: entry.participation_type,
                    group_name: entry.group_name,
                    team_star: entry.team_star,
                }),
                Err(error) => {
                    warn!(%error, %field, %contest_id, "ignoring malformed frozen roster entry")
                }
            }
        }
        Ok(Some(roster))
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
        let max_score_milli = sqlx::query_scalar::<_, i32>(
            "SELECT max_score_milli FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_optional(database)
        .await?;
        // A cell for a problem no longer assigned to this contest has nothing
        // to replay; `max_score_milli` is NOT NULL, so a missing row is the
        // only "absent" case. Never default it to zero: in POINTS mode that
        // makes any score look like full marks and flips `solved`.
        let Some(max_score_milli) = max_score_milli.map(i64::from) else { return Ok(()) };
        let rows = load_cell_replay_rows(database, contest_id, team_id, problem_id).await?;
        let Some(start_at) = meta.0 else { return Ok(()) };
        let replay: Vec<ReplayRow> = rows
            .into_iter()
            .map(|(submission_id, verdict, score_milli, submitted_at)| ReplayRow {
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
        let rows = load_contest_replay_rows(database, contest_id, None).await?;
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
        // Build into a staging key and publish it with a single RENAME. The old
        // `DEL cells → HSET …` pipeline could be interrupted after the DEL,
        // leaving an empty board that the very same pipeline then blessed with
        // a fresh version — readers trust a freshly versioned hash.
        let staging = format!("{CELLS_KEY_TEMPLATE}:building:{contest_id}");
        let mut pipeline = redis::pipe();
        pipeline.del(&staging).ignore();
        for ((team_id, problem_id), (max_score, replay)) in &grouped {
            let state = project_cell(start_ms, meta.1, meta.2, *max_score, replay);
            pipeline
                .hset(
                    &staging,
                    cell_field(*team_id, *problem_id),
                    serde_json::to_string(&state).map_err(|error| {
                        sqlx::Error::Protocol(format!("encode cell: {error:?}"))
                    })?,
                )
                .ignore();
        }
        if grouped.is_empty() {
            // Nothing to publish: an empty contest must not keep a stale hash.
            pipeline.del(cells_key(contest_id)).ignore();
        } else {
            // The TTL only matters if the pipeline dies before the RENAME.
            pipeline.cmd("EXPIRE").arg(&staging).arg(300).ignore();
            pipeline.cmd("RENAME").arg(&staging).arg(cells_key(contest_id)).ignore();
        }
        pipeline.incr(version_key(contest_id), 1).ignore();
        pipeline.del(dirty_key(contest_id)).ignore();
        pipeline.del(untrusted_key(contest_id)).ignore();
        self.redis.query_pipeline::<()>(pipeline).await.map_err(|error| {
            sqlx::Error::Protocol(format!("rebuild contest projection: {error:?}"))
        })
    }

    /// Materialises the frozen snapshot as a replay of the contest **as of
    /// `freeze_at`**.
    ///
    /// Copying the live cells hash (the previous behaviour) made the snapshot
    /// depend on when the milestone happened to be noticed: a restart around
    /// the freeze, or a projection enabled afterwards, would capture
    /// post-freeze judgements and leak them on the public board for the rest of
    /// the contest. Replaying only submissions before the cutoff makes the
    /// snapshot a function of the milestone, not of the timing.
    ///
    /// Idempotent: `RENAMENX` publishes the first snapshot only, so a second
    /// call (or a concurrent one) leaves the established snapshot intact.
    pub async fn freeze_snapshot(
        &self,
        database: &PgPool,
        contest_id: i64,
    ) -> Result<(), AppError> {
        let exists: i64 = self.redis.query(cmd("EXISTS").arg(frozen_key(contest_id))).await?;
        if exists != 0 {
            return Ok(());
        }
        let meta =
            sqlx::query_as::<_, (Option<OffsetDateTime>, Option<OffsetDateTime>, bool, bool)>(
                r#"
            SELECT c.start_at, c.freeze_at, c.scoring_mode = 'ICPC',
                   c.score_aggregation = 'BEST'
            FROM contests c WHERE c.id = $1
            "#,
            )
            .bind(contest_id)
            .fetch_one(database)
            .await
            .map_err(|error| AppError::internal("load contest for the frozen snapshot", error))?;
        let (Some(start_at), Some(freeze_at)) = (meta.0, meta.1) else { return Ok(()) };
        let start_ms = unix_ms(start_at);
        // Snapshot the roster too: the frozen board must show the names, groups
        // and STAR flags that were public when it froze, not whatever the teams
        // table says after the contest.
        let roster =
            sqlx::query_as::<_, (i64, String, Option<String>, String, Option<String>, bool)>(
                r#"
            SELECT roster.team_id, team.name, team.school, roster.participation_type,
                   roster.group_name, team.star
            FROM contest_teams roster
            JOIN teams team ON team.id = roster.team_id
            WHERE roster.contest_id = $1
            "#,
            )
            .bind(contest_id)
            .fetch_all(database)
            .await
            .map_err(|error| AppError::internal("load roster for the frozen snapshot", error))?;
        let rows = load_contest_replay_rows(database, contest_id, Some(freeze_at))
            .await
            .map_err(|error| AppError::internal("replay the frozen snapshot", error))?;
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
        // Build into a staging key and publish it with an atomic RENAMENX so a
        // reader never observes a half-written snapshot.
        let staging = format!("{FROZEN_KEY_TEMPLATE}:building:{contest_id}");
        let mut pipeline = redis::pipe();
        pipeline.del(&staging).ignore();
        for ((team_id, problem_id), (max_score, replay)) in &grouped {
            let state = project_cell(start_ms, meta.2, meta.3, *max_score, replay);
            let payload = serde_json::to_string(&state)
                .map_err(|error| AppError::internal("encode frozen cell", error))?;
            pipeline.hset(&staging, cell_field(*team_id, *problem_id), payload).ignore();
        }
        pipeline.cmd("RENAMENX").arg(&staging).arg(frozen_key(contest_id)).ignore();
        pipeline.del(&staging).ignore();
        let roster_staging = format!("{FROZEN_ROSTER_KEY_TEMPLATE}:building:{contest_id}");
        pipeline.del(&roster_staging).ignore();
        for (team_id, team_name, school, participation_type, group_name, team_star) in roster {
            let entry =
                FrozenRosterEntry { team_name, school, participation_type, group_name, team_star };
            let payload = serde_json::to_string(&entry)
                .map_err(|error| AppError::internal("encode frozen roster entry", error))?;
            pipeline.hset(&roster_staging, team_id.to_string(), payload).ignore();
        }
        pipeline.cmd("RENAMENX").arg(&roster_staging).arg(frozen_roster_key(contest_id)).ignore();
        pipeline.del(&roster_staging).ignore();
        self.redis.query_pipeline::<()>(pipeline).await?;
        if grouped.is_empty() {
            warn!(%contest_id, "no submissions before the freeze; the frozen board is empty");
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

/// Loads one cell's completed judgement history for replay.
///
/// `judgements.score_milli` is `integer` in PostgreSQL, while [`ReplayRow`]
/// keeps an `i64`; the explicit `::bigint` cast keeps the SQL type and the
/// Rust decode type aligned. sqlx's PostgreSQL decoder accepts INT8 for
/// `i64` but not INT4, so a missing cast makes every non-empty cell fail to
/// decode — breaking both rejudge repair and the post-Redis-loss rebuild.
pub(super) async fn load_cell_replay_rows(
    database: &PgPool,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
) -> Result<Vec<(i64, String, i64, OffsetDateTime)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String, i64, OffsetDateTime)>(
        r#"
        SELECT s.id, j.verdict, coalesce(j.score_milli, 0)::bigint, s.submitted_at
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
    .await
}

/// Loads every completed judgement of a contest for a full projection replay.
/// Both score columns are `integer` in PostgreSQL, so they are cast to
/// `bigint` for the `i64` decode (see [`load_cell_replay_rows`]).
/// `until` restricts the replay to submissions strictly before that instant,
/// which is how the frozen snapshot is built.
pub(super) async fn load_contest_replay_rows(
    database: &PgPool,
    contest_id: i64,
    until: Option<OffsetDateTime>,
) -> Result<Vec<(i64, i64, i64, OffsetDateTime, String, i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, i64, i64, OffsetDateTime, String, i64, i64)>(
        r#"
        SELECT s.team_id, s.problem_id, s.id, s.submitted_at, j.verdict,
               coalesce(j.score_milli, 0)::bigint, coalesce(cp.max_score_milli, 0)::bigint
        FROM submissions s
        JOIN judgements j ON j.submission_id = s.id
            AND j.active_marker IS TRUE AND j.completed_at IS NOT NULL
        JOIN contest_problems cp ON cp.contest_id = s.contest_id
            AND cp.problem_id = s.problem_id
        WHERE s.contest_id = $1
          AND ($2::timestamptz IS NULL OR s.submitted_at < $2)
        ORDER BY s.submitted_at, s.id
        "#,
    )
    .bind(contest_id)
    .bind(until)
    .fetch_all(database)
    .await
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
            // Mirrors the POINTS branch of `APPLY_SCRIPT`: only the newest
            // submission may revoke a solve or overwrite a LAST score, so an
            // out-of-order judgement cannot rewrite the board.
            let newest = t >= cell.last_at;
            if aggregation_best {
                if row.score_milli > cell.score_milli {
                    cell.score_milli = row.score_milli;
                }
            } else if newest {
                cell.score_milli = row.score_milli;
            }
            if cell.score_milli >= max_score_milli {
                if !cell.solved {
                    cell.solved = true;
                    cell.solved_at = t;
                } else if t < cell.solved_at {
                    cell.solved_at = t;
                }
            } else if newest {
                cell.solved = false;
                cell.solved_at = 0;
            }
            if newest {
                cell.last_at = t;
            }
            cell.penalty = 0;
            // Mirrors `APPLY_SCRIPT`: POINTS boards expose every attempt, so the
            // live projection agrees with the PostgreSQL rebuild.
            cell.wa = cell.attempts;
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

    #[test]
    fn points_replay_reports_every_attempt() {
        // The PostgreSQL rebuild counts every completed judgement in POINTS
        // mode, so the replay (and, through `APPLY_SCRIPT`, the incremental
        // path) must expose the same number instead of zero.
        let start = 1_000_000;
        let rows = [
            row(60_000, "WRONG_ANSWER", 0),
            row(120_000, "TIME_LIMIT_EXCEEDED", 0),
            row(180_000, "ACCEPTED", 10_000),
        ];
        let cell = project_cell(start, false, true, 10_000, &rows);
        assert_eq!(cell.attempts, 3);
        assert_eq!(cell.wa, 3, "a POINTS board shows all attempts");
        assert_eq!(to_cell_row(1, 1, &cell).wrong_attempts, 3);

        // ICPC keeps counting only penalised rejections before the solve.
        let icpc = project_cell(start, true, true, 10_000, &rows);
        assert_eq!(icpc.wa, 2);
    }
}
