use std::net::IpAddr;

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::{error::AppError, features::auth::model::AuthUser};

use super::ScoreboardCache;
use super::helpers::{apply_scoreboard_filter, assemble, contest_not_found, score_submissions};
use super::model::{
    CellRow, ContestBoardRow, RosterRow, ScoreboardProblem, ScoreboardResponse,
    ScoreboardSnapshotResponse, SnapshotRow, SubmissionScoreRow, ValidatedScoreboardQuery,
    ValidatedSnapshotSelector,
};

pub struct ScoreboardService {
    database: PgPool,
    cache: Option<ScoreboardCache>,
}

impl ScoreboardService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database, cache: None }
    }

    #[must_use]
    pub fn with_cache(mut self, cache: ScoreboardCache) -> Self {
        self.cache = Some(cache);
        self
    }

    pub async fn public(
        &self,
        contest_id: i64,
        query: ValidatedScoreboardQuery,
    ) -> Result<ScoreboardResponse, AppError> {
        self.load(contest_id, query, false).await
    }

    pub async fn admin(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        query: ValidatedScoreboardQuery,
    ) -> Result<ScoreboardResponse, AppError> {
        self.require_admin_access(contest_id, actor).await?;
        self.load(contest_id, query, true).await
    }

    pub async fn create_snapshot(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
        selector: ValidatedSnapshotSelector,
    ) -> Result<ScoreboardSnapshotResponse, AppError> {
        self.require_admin_access(contest_id, actor).await?;
        let group_name = selector.query.group_name.clone();
        let participation_type = selector.query.participation_type.clone();
        let board = self.load(contest_id, selector.query, selector.variant == "ADMIN").await?;
        let payload_json = serde_json::to_string(&board)
            .map_err(|error| AppError::internal("encode scoreboard snapshot", error))?;
        let payload_sha256 = hex::encode(Sha256::digest(payload_json.as_bytes()));
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin scoreboard snapshot", error))?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(contest_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock scoreboard snapshot version", error))?;
        let version = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT coalesce(max(version), 0) + 1
            FROM scoreboard_snapshots
            WHERE contest_id = $1
              AND variant = $2
              AND group_name IS NOT DISTINCT FROM $3
              AND participation_type IS NOT DISTINCT FROM $4
            "#,
        )
        .bind(contest_id)
        .bind(selector.variant)
        .bind(group_name.as_deref())
        .bind(participation_type.as_deref())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("allocate scoreboard snapshot version", error))?;
        let row = sqlx::query_as::<_, SnapshotRow>(
            r#"
            INSERT INTO scoreboard_snapshots (
                contest_id, variant, group_name, participation_type, version, frozen,
                generated_at, payload_json, payload_sha256, created_by, created_by_user_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, contest_id, variant, group_name, participation_type, version,
                      frozen, generated_at, payload_json, payload_sha256
            "#,
        )
        .bind(contest_id)
        .bind(selector.variant)
        .bind(group_name.as_deref())
        .bind(participation_type.as_deref())
        .bind(version)
        .bind(board.frozen)
        .bind(board.generated_at)
        .bind(payload_json)
        .bind(payload_sha256)
        .bind(&actor.username)
        .bind(actor.id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("persist scoreboard snapshot", error))?;
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (actor_user_id, action, target_type, target_id, request_ip, result)
            VALUES ($1, 'SCOREBOARD_SNAPSHOT_CREATED', 'SCOREBOARD_SNAPSHOT', $2, $3, 'success')
            "#,
        )
        .bind(actor.id)
        .bind(row.id.to_string())
        .bind(request_ip.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("record scoreboard snapshot audit", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit scoreboard snapshot", error))?;
        row.response()
    }

    pub async fn latest_snapshot(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        selector: ValidatedSnapshotSelector,
    ) -> Result<ScoreboardSnapshotResponse, AppError> {
        self.require_admin_access(contest_id, actor).await?;
        sqlx::query_as::<_, SnapshotRow>(
            r#"
            SELECT id, contest_id, variant, group_name, participation_type, version,
                   frozen, generated_at, payload_json, payload_sha256
            FROM scoreboard_snapshots
            WHERE contest_id = $1
              AND variant = $2
              AND group_name IS NOT DISTINCT FROM $3
              AND participation_type IS NOT DISTINCT FROM $4
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(contest_id)
        .bind(selector.variant)
        .bind(selector.query.group_name.as_deref())
        .bind(selector.query.participation_type.as_deref())
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load latest scoreboard snapshot", error))?
        .ok_or_else(|| {
            AppError::not_found("SCOREBOARD_SNAPSHOT_NOT_FOUND", "Snapshot was not found")
        })?
        .response()
    }

    async fn require_admin_access(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<(), AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        if actor.is_super_admin() {
            return Ok(());
        }
        let assigned = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM contest_management_assignments
                WHERE contest_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(contest_id)
        .bind(actor.id)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("check scoreboard administrator scope", error))?;
        if assigned { Ok(()) } else { Err(contest_not_found()) }
    }

    async fn load(
        &self,
        contest_id: i64,
        query: ValidatedScoreboardQuery,
        admin: bool,
    ) -> Result<ScoreboardResponse, AppError> {
        let contest = sqlx::query_as::<_, ContestBoardRow>(
            r#"
            SELECT status, start_at, freeze_at, end_at, scoreboard_revision,
                   scoring_mode, score_aggregation
            FROM contests
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(contest_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load scoreboard contest", error))?
        .ok_or_else(contest_not_found)?;
        let start_at = contest.start_at.ok_or_else(|| {
            AppError::conflict("CONTEST_SCHEDULE_REQUIRED", "Contest schedule is not configured")
        })?;
        let generated_at = OffsetDateTime::now_utc();
        let frozen = !admin
            && matches!(contest.status.as_str(), "RUNNING" | "PAUSED")
            && contest.freeze_at.is_some_and(|freeze_at| freeze_at <= generated_at)
            && contest.end_at.is_some_and(|end_at| generated_at < end_at);
        let variant = if admin { "ADMIN" } else { "PUBLIC" };
        let phase = if admin {
            "ADMIN"
        } else if frozen {
            "FROZEN"
        } else if contest.end_at.is_some_and(|end_at| generated_at >= end_at) {
            "FINAL"
        } else {
            "LIVE"
        };
        if let Some(cache) = &self.cache
            && let Some(board) =
                cache.get(contest_id, contest.scoreboard_revision, variant, phase, &query).await
        {
            return Ok(board);
        }
        // Load the complete roster before applying the requested view filter.
        // First-blood is a contest-wide fact and must not be recomputed from a
        // filtered subset of teams.
        let roster = self
            .load_roster(
                contest_id,
                &ValidatedScoreboardQuery { group_name: None, participation_type: None },
            )
            .await?;
        let problems = sqlx::query_as::<_, ScoreboardProblem>(
            r#"
            SELECT problem_id, alias, display_order
            FROM contest_problems
            WHERE contest_id = $1
            ORDER BY display_order, problem_id
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load scoreboard problems", error))?;
        let cells = if frozen {
            self.calculate_frozen_cells(
                contest_id,
                start_at,
                contest.freeze_at.ok_or_else(|| {
                    AppError::internal_message(
                        "load frozen scoreboard",
                        "freeze timestamp disappeared",
                    )
                })?,
                &contest.scoring_mode,
                &contest.score_aggregation,
            )
            .await?
        } else {
            self.load_live_cells(contest_id).await?
        };
        let mut board = assemble(
            contest_id,
            variant,
            frozen,
            generated_at,
            contest.scoring_mode,
            contest.score_aggregation,
            problems,
            roster,
            cells,
        );
        apply_scoreboard_filter(&mut board, &query);
        if let Some(cache) = &self.cache {
            cache.put(contest.scoreboard_revision, phase, &query, &board).await;
        }
        Ok(board)
    }

    async fn load_roster(
        &self,
        contest_id: i64,
        query: &ValidatedScoreboardQuery,
    ) -> Result<Vec<RosterRow>, AppError> {
        sqlx::query_as::<_, RosterRow>(
            r#"
            SELECT roster.team_id,
                   team.name AS team_name,
                   team.school,
                   roster.participation_type,
                   roster.group_name,
                   team.star AS team_star
            FROM contest_teams roster
            JOIN teams team ON team.id = roster.team_id
            WHERE roster.contest_id = $1
              AND team.deleted_at IS NULL
              AND ($2::text IS NULL OR roster.group_name = $2)
              AND ($3::text IS NULL OR roster.participation_type = $3)
            "#,
        )
        .bind(contest_id)
        .bind(query.group_name.as_deref())
        .bind(query.participation_type.as_deref())
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load scoreboard roster", error))
    }

    async fn load_live_cells(&self, contest_id: i64) -> Result<Vec<CellRow>, AppError> {
        sqlx::query_as::<_, CellRow>(
            r#"
            SELECT team_id, problem_id, wrong_attempts, solved, solved_at, penalty_minutes,
                   score_milli
            FROM contest_scoreboard_cells
            WHERE contest_id = $1
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load live scoreboard cells", error))
    }

    async fn calculate_frozen_cells(
        &self,
        contest_id: i64,
        start_at: OffsetDateTime,
        cutoff: OffsetDateTime,
        scoring_mode: &str,
        score_aggregation: &str,
    ) -> Result<Vec<CellRow>, AppError> {
        let submissions = sqlx::query_as::<_, SubmissionScoreRow>(
            r#"
            SELECT submission.id AS submission_id,
                   submission.team_id,
                   submission.problem_id,
                   submission.submitted_at,
                   judgement.verdict,
                   judgement.score_milli,
                   assignment.max_score_milli
            FROM submissions submission
            JOIN judgements judgement
              ON judgement.submission_id = submission.id
             AND judgement.active_marker IS TRUE
             AND judgement.completed_at IS NOT NULL
            JOIN contest_problems assignment
              ON assignment.contest_id=submission.contest_id
             AND assignment.problem_id=submission.problem_id
            WHERE submission.contest_id = $1
              AND submission.submitted_at < $2
            ORDER BY submission.submitted_at, submission.id
            "#,
        )
        .bind(contest_id)
        .bind(cutoff)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("calculate frozen scoreboard", error))?;
        Ok(score_submissions(start_at, scoring_mode, score_aggregation, submissions))
    }
}
