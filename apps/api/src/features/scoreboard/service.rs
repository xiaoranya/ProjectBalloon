use std::{cmp::Ordering, collections::HashMap, net::IpAddr};

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::{error::AppError, features::auth::model::AuthUser};

use super::ScoreboardCache;
use super::model::{
    CellRow, ContestBoardRow, RosterRow, ScoreboardCell, ScoreboardProblem, ScoreboardResponse,
    ScoreboardRow, ScoreboardSnapshotResponse, SnapshotRow, SubmissionScoreRow,
    ValidatedScoreboardQuery, ValidatedSnapshotSelector,
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
        if actor.has_role("SUPER_ADMIN") {
            return Ok(());
        }
        let assigned = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM contest_admin_assignments
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
                    AppError::internal("load frozen scoreboard", "freeze timestamp disappeared")
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

fn apply_scoreboard_filter(board: &mut ScoreboardResponse, query: &ValidatedScoreboardQuery) {
    board.rows.retain(|row| {
        query.group_name.as_ref().is_none_or(|group| row.group_name.as_ref() == Some(group))
            && query
                .participation_type
                .as_ref()
                .is_none_or(|participation| &row.participation_type == participation)
    });
    board.rows.sort_by(|left, right| compare_rows(&board.scoring_mode, left, right));
    let mut official_rank = 0_u32;
    for (index, row) in board.rows.iter_mut().enumerate() {
        row.rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        row.official_rank = if row.participation_type == "OFFICIAL" {
            official_rank = official_rank.saturating_add(1);
            Some(official_rank)
        } else {
            None
        };
    }
}

fn score_submissions(
    start_at: OffsetDateTime,
    scoring_mode: &str,
    score_aggregation: &str,
    submissions: Vec<SubmissionScoreRow>,
) -> Vec<CellRow> {
    let mut cells: HashMap<(i64, i64), CellRow> = HashMap::new();
    for submission in submissions {
        let cell = cells.entry((submission.team_id, submission.problem_id)).or_insert(CellRow {
            team_id: submission.team_id,
            problem_id: submission.problem_id,
            wrong_attempts: 0,
            solved: false,
            solved_at: None,
            penalty_minutes: 0,
            score_milli: 0,
        });
        if scoring_mode != "ICPC" {
            cell.wrong_attempts = cell.wrong_attempts.saturating_add(1);
            let replaces = score_aggregation == "LAST" || submission.score_milli > cell.score_milli;
            if replaces {
                cell.score_milli = submission.score_milli;
                cell.solved = submission.score_milli >= submission.max_score_milli;
                cell.solved_at = cell.solved.then_some(submission.submitted_at);
                cell.penalty_minutes = 0;
            }
            continue;
        }
        if cell.solved {
            continue;
        }
        if submission.verdict == "ACCEPTED" {
            let elapsed_minutes = (submission.submitted_at - start_at).whole_minutes().max(0);
            cell.solved = true;
            cell.solved_at = Some(submission.submitted_at);
            cell.penalty_minutes = elapsed_minutes + 20 * i64::from(cell.wrong_attempts);
            cell.score_milli = submission.max_score_milli;
        } else if is_penalized_rejection(&submission.verdict) {
            cell.wrong_attempts = cell.wrong_attempts.saturating_add(1);
        }
        let _submission_id = submission.submission_id;
    }
    cells.into_values().collect()
}

fn is_penalized_rejection(verdict: &str) -> bool {
    matches!(
        verdict,
        "WRONG_ANSWER"
            | "TIME_LIMIT_EXCEEDED"
            | "MEMORY_LIMIT_EXCEEDED"
            | "RUNTIME_ERROR"
            | "OUTPUT_LIMIT_EXCEEDED"
    )
}

// Projection inputs stay explicit because each value participates in the cache key.
#[allow(clippy::too_many_arguments)]
fn assemble(
    contest_id: i64,
    variant: &'static str,
    frozen: bool,
    generated_at: OffsetDateTime,
    scoring_mode: String,
    score_aggregation: String,
    mut problems: Vec<ScoreboardProblem>,
    roster: Vec<RosterRow>,
    cells: Vec<CellRow>,
) -> ScoreboardResponse {
    let cells: HashMap<(i64, i64), CellRow> =
        cells.into_iter().map(|cell| ((cell.team_id, cell.problem_id), cell)).collect();
    let mut rows: Vec<ScoreboardRow> = roster
        .into_iter()
        .map(|team| {
            let problem_cells: Vec<ScoreboardCell> = problems
                .iter()
                .map(|problem| {
                    cells.get(&(team.team_id, problem.problem_id)).map_or(
                        ScoreboardCell {
                            problem_id: problem.problem_id,
                            wrong_attempts: 0,
                            solved: false,
                            solved_at: None,
                            penalty_minutes: 0,
                            score_milli: 0,
                            first_blood: false,
                        },
                        |cell| ScoreboardCell {
                            problem_id: problem.problem_id,
                            wrong_attempts: cell.wrong_attempts,
                            solved: cell.solved,
                            solved_at: cell.solved_at,
                            penalty_minutes: cell.penalty_minutes,
                            score_milli: cell.score_milli,
                            first_blood: false,
                        },
                    )
                })
                .collect();
            let solved_count =
                i32::try_from(problem_cells.iter().filter(|cell| cell.solved).count())
                    .unwrap_or(i32::MAX);
            let penalty_minutes = problem_cells
                .iter()
                .filter(|cell| cell.solved)
                .map(|cell| cell.penalty_minutes)
                .sum();
            let total_score_milli =
                problem_cells.iter().map(|cell| i64::from(cell.score_milli)).sum();
            let last_solved_at = problem_cells.iter().filter_map(|cell| cell.solved_at).max();
            ScoreboardRow {
                rank: 0,
                official_rank: None,
                team_id: team.team_id,
                team_name: team.team_name,
                school: team.school,
                is_star: team.team_star || team.participation_type == "STAR",
                participation_type: team.participation_type,
                group_name: team.group_name,
                solved_count,
                penalty_minutes,
                total_score_milli,
                last_solved_at,
                problems: problem_cells,
            }
        })
        .collect();
    rows.sort_by(|left, right| compare_rows(&scoring_mode, left, right));
    let mut official_rank = 0_u32;
    for (index, row) in rows.iter_mut().enumerate() {
        row.rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if row.participation_type == "OFFICIAL" {
            official_rank = official_rank.saturating_add(1);
            row.official_rank = Some(official_rank);
        }
    }
    for problem in &mut problems {
        let first_blood = rows
            .iter()
            .filter(|row| row.participation_type != "PRACTICE")
            .filter_map(|row| {
                row.problems
                    .iter()
                    .find(|cell| cell.problem_id == problem.problem_id && cell.solved)
                    .and_then(|cell| cell.solved_at.map(|solved_at| (solved_at, row.team_id)))
            })
            .min();
        if let Some((solved_at, team_id)) = first_blood {
            problem.first_blood_at = Some(solved_at);
            problem.first_blood_team_id = Some(team_id);
            if let Some(cell) = rows.iter_mut().find(|row| row.team_id == team_id).and_then(|row| {
                row.problems.iter_mut().find(|cell| cell.problem_id == problem.problem_id)
            }) {
                cell.first_blood = true;
            }
        }
    }
    ScoreboardResponse {
        contest_id,
        variant: variant.to_owned(),
        frozen,
        scoring_mode,
        score_aggregation,
        generated_at,
        problems,
        rows,
    }
}

fn compare_rows(scoring_mode: &str, left: &ScoreboardRow, right: &ScoreboardRow) -> Ordering {
    if scoring_mode != "ICPC" {
        return right
            .total_score_milli
            .cmp(&left.total_score_milli)
            .then_with(|| match (left.last_solved_at, right.last_solved_at) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| left.team_id.cmp(&right.team_id));
    }
    right
        .solved_count
        .cmp(&left.solved_count)
        .then_with(|| left.penalty_minutes.cmp(&right.penalty_minutes))
        .then_with(|| match (left.last_solved_at, right.last_solved_at) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| left.team_id.cmp(&right.team_id))
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

pub(crate) fn to_csv(board: &ScoreboardResponse) -> String {
    let mut output = String::from(
        "rank,officialRank,teamId,teamName,school,participationType,groupName,solvedCount,penaltyMinutes",
    );
    for problem in &board.problems {
        output.push(',');
        output.push_str(&csv_field(&problem.alias));
    }
    output.push('\n');
    for row in &board.rows {
        let fields = [
            row.rank.to_string(),
            row.official_rank.map_or_else(String::new, |rank| rank.to_string()),
            row.team_id.to_string(),
            row.team_name.clone(),
            row.school.clone().unwrap_or_default(),
            row.participation_type.clone(),
            row.group_name.clone().unwrap_or_default(),
            row.solved_count.to_string(),
            row.penalty_minutes.to_string(),
        ];
        output.push_str(&fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","));
        for cell in &row.problems {
            output.push(',');
            let value = if cell.solved {
                // OI/IOI cells carry no penalty-based solve time, so rendering
                // `@minutes` would produce a negative value. Show the attempt
                // count only for non-ICPC modes.
                if board.scoring_mode == "ICPC" {
                    let solve_minutes = cell.penalty_minutes - 20 * i64::from(cell.wrong_attempts);
                    format!("+{}@{solve_minutes}", cell.wrong_attempts)
                } else {
                    format!("+{}", cell.wrong_attempts)
                }
            } else if cell.wrong_attempts > 0 {
                format!("-{}", cell.wrong_attempts)
            } else {
                String::new()
            };
            output.push_str(&csv_field(&value));
        }
        output.push('\n');
    }
    output
}

fn csv_field(value: &str) -> String {
    if value.chars().any(|character| matches!(character, ',' | '"' | '\r' | '\n')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration as StdDuration;

    use redis::AsyncCommands;
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use crate::features::{
        auth::model::{AuthUser, UserType},
        scoreboard::{ScoreboardCache, projection::rebuild_cell},
    };

    use super::{
        ScoreboardService, SubmissionScoreRow, ValidatedScoreboardQuery, ValidatedSnapshotSelector,
        score_submissions, to_csv,
    };

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn public_freeze_hides_late_acceptance_while_admin_sees_true_board(pool: PgPool) {
        let now = OffsetDateTime::now_utc();
        let start_at = now - Duration::hours(2);
        let freeze_at = start_at + Duration::hours(1);
        let end_at = now + Duration::hours(1);
        let contest_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO contests
                (name, status, visibility, start_at, freeze_at, end_at)
            VALUES ('Frozen Board', 'RUNNING', 'PUBLIC', $1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(start_at)
        .bind(freeze_at)
        .bind(end_at)
        .fetch_one(&pool)
        .await
        .expect("insert contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title) VALUES ('board-a', 'Board A') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert problem");
        sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
        let mut teams = Vec::new();
        for (name, participation_type) in
            [("Official, Team", "OFFICIAL"), ("Star Team", "STAR"), ("Practice Team", "PRACTICE")]
        {
            let team_id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO teams (name, school) VALUES ($1, 'XCPC University') RETURNING id",
            )
            .bind(name)
            .fetch_one(&pool)
            .await
            .expect("insert team");
            sqlx::query(
                r#"
                INSERT INTO contest_teams
                    (contest_id, team_id, participation_type, group_name)
                VALUES ($1, $2, $3, 'East')
                "#,
            )
            .bind(contest_id)
            .bind(team_id)
            .bind(participation_type)
            .execute(&pool)
            .await
            .expect("roster team");
            teams.push(team_id);
        }
        insert_scored_submission(
            &pool,
            contest_id,
            problem_id,
            teams[0],
            start_at + Duration::minutes(10),
            "WRONG_ANSWER",
        )
        .await;
        insert_scored_submission(
            &pool,
            contest_id,
            problem_id,
            teams[0],
            start_at + Duration::minutes(50),
            "ACCEPTED",
        )
        .await;
        insert_scored_submission(
            &pool,
            contest_id,
            problem_id,
            teams[1],
            start_at + Duration::minutes(70),
            "ACCEPTED",
        )
        .await;
        let mut transaction = pool.begin().await.expect("begin projection transaction");
        rebuild_cell(&mut transaction, contest_id, teams[0], problem_id)
            .await
            .expect("project official team");
        rebuild_cell(&mut transaction, contest_id, teams[1], problem_id)
            .await
            .expect("project star team");
        transaction.commit().await.expect("commit projections");

        let service = ScoreboardService::new(pool.clone());
        let query = || ValidatedScoreboardQuery { group_name: None, participation_type: None };
        let public = service.public(contest_id, query()).await.expect("load public board");
        assert!(public.frozen);
        assert_eq!(public.rows.len(), 3);
        assert_eq!(public.rows[0].team_id, teams[0]);
        assert_eq!((public.rows[0].solved_count, public.rows[0].penalty_minutes), (1, 70));
        assert_eq!(public.rows[0].official_rank, Some(1));
        assert!(public.rows[0].problems[0].first_blood);
        assert_eq!(public.problems[0].first_blood_team_id, Some(teams[0]));
        assert_eq!(public.rows[1].solved_count, 0);
        assert_eq!(public.rows[2].solved_count, 0);

        let admin_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('snapshot-root', 'test-only-hash', 'Snapshot Root', 'SUPER_ADMIN')
            RETURNING id
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("insert snapshot creator");
        let admin = AuthUser {
            id: admin_id,
            username: "root".to_owned(),
            display_name: "Root".to_owned(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
            password_reset_required: false,
        };
        let true_board =
            service.admin(contest_id, &admin, query()).await.expect("load admin board");
        assert!(!true_board.frozen);
        assert_eq!(true_board.rows[0].team_id, teams[0]);
        assert_eq!(true_board.rows[1].team_id, teams[1]);
        assert_eq!(true_board.rows[1].solved_count, 1);
        assert_eq!(true_board.rows[1].official_rank, None);
        assert_eq!(true_board.rows[2].team_id, teams[2]);
        let csv = to_csv(&true_board);
        assert!(csv.contains("\"Official, Team\""));
        assert!(csv.contains("+1@50"));
        let snapshot_selector = || ValidatedSnapshotSelector {
            variant: "PUBLIC",
            query: ValidatedScoreboardQuery {
                group_name: Some("East".to_owned()),
                participation_type: None,
            },
        };
        let first_snapshot = service
            .create_snapshot(
                contest_id,
                &admin,
                "127.0.0.1".parse().expect("loopback IP"),
                snapshot_selector(),
            )
            .await
            .expect("create first immutable snapshot");
        let second_snapshot = service
            .create_snapshot(
                contest_id,
                &admin,
                "127.0.0.1".parse().expect("loopback IP"),
                snapshot_selector(),
            )
            .await
            .expect("create second immutable snapshot");
        assert_eq!((first_snapshot.version, second_snapshot.version), (1, 2));
        assert_eq!(first_snapshot.payload_sha256.len(), 64);
        assert_eq!(first_snapshot.payload["frozen"], true);
        let latest = service
            .latest_snapshot(contest_id, &admin, snapshot_selector())
            .await
            .expect("load latest immutable snapshot");
        assert_eq!(latest.id, second_snapshot.id);
        let snapshot_audits = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE action = 'SCOREBOARD_SNAPSHOT_CREATED' AND actor_user_id = $1",
        )
        .bind(admin.id)
        .fetch_one(&pool)
        .await
        .expect("count snapshot audits");
        assert_eq!(snapshot_audits, 2);
        let mutation = sqlx::query("UPDATE scoreboard_snapshots SET frozen = false WHERE id = $1")
            .bind(first_snapshot.id)
            .execute(&pool)
            .await;
        assert!(mutation.is_err(), "database must reject snapshot mutation");
        let accepted_submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT submission.id
            FROM submissions submission
            JOIN judgements judgement ON judgement.submission_id = submission.id
            WHERE submission.contest_id = $1
              AND submission.team_id = $2
              AND submission.problem_id = $3
              AND judgement.active_marker IS TRUE
              AND judgement.verdict = 'ACCEPTED'
            "#,
        )
        .bind(contest_id)
        .bind(teams[0])
        .bind(problem_id)
        .fetch_one(&pool)
        .await
        .expect("find accepted submission for rejudge");
        sqlx::query(
            r#"
            UPDATE judgements
            SET superseded = true, active_marker = NULL
            WHERE submission_id = $1 AND active_marker IS TRUE
            "#,
        )
        .bind(accepted_submission_id)
        .execute(&pool)
        .await
        .expect("supersede accepted judgement");
        sqlx::query(
            r#"
            INSERT INTO judgements (id, submission_id, verdict, completed_at)
            VALUES ($1, $2, 'WRONG_ANSWER', now())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(accepted_submission_id)
        .execute(&pool)
        .await
        .expect("insert replacement judgement");
        let mut transaction = pool.begin().await.expect("begin rejudge projection");
        rebuild_cell(&mut transaction, contest_id, teams[0], problem_id)
            .await
            .expect("rebuild rejudged scoreboard cell");
        transaction.commit().await.expect("commit rejudge projection");
        let rolled_back = service
            .admin(contest_id, &admin, query())
            .await
            .expect("load scoreboard after AC rollback");
        assert_eq!(rolled_back.rows[0].team_id, teams[1]);
        assert_eq!(rolled_back.rows[0].solved_count, 1);
        assert!(rolled_back.rows[0].problems[0].first_blood);
        let official = rolled_back
            .rows
            .iter()
            .find(|row| row.team_id == teams[0])
            .expect("official team remains on board");
        assert_eq!((official.solved_count, official.penalty_minutes), (0, 0));
        assert_eq!(official.problems[0].wrong_attempts, 2);
        let persisted_snapshot = service
            .latest_snapshot(contest_id, &admin, snapshot_selector())
            .await
            .expect("snapshot survives live rejudge");
        assert_eq!(persisted_snapshot.payload["rows"][0]["teamId"], teams[0]);
        assert_eq!(persisted_snapshot.payload["rows"][0]["solvedCount"], 1);

        let stars = service
            .public(
                contest_id,
                ValidatedScoreboardQuery {
                    group_name: Some("East".to_owned()),
                    participation_type: Some("STAR".to_owned()),
                },
            )
            .await
            .expect("filter star board");
        assert_eq!(stars.rows.len(), 1);
        assert_eq!(stars.rows[0].team_id, teams[1]);
        assert_eq!(stars.rows[0].solved_count, 0);

        if let Ok(redis_url) = std::env::var("PROJECT_BALLOON_TEST_REDIS_URL") {
            let cache = ScoreboardCache::connect(
                &redis_url,
                StdDuration::from_secs(300),
                StdDuration::from_millis(500),
            )
            .await
            .expect("connect scoreboard test Redis");
            let cached_service = ScoreboardService::new(pool.clone()).with_cache(cache);
            let first = cached_service
                .admin(contest_id, &admin, query())
                .await
                .expect("populate scoreboard cache");
            let hit = cached_service
                .admin(contest_id, &admin, query())
                .await
                .expect("read scoreboard cache hit");
            assert_eq!(hit.generated_at, first.generated_at);

            let client = redis::Client::open(redis_url).expect("open scoreboard test Redis");
            let mut redis = client
                .get_multiplexed_async_connection()
                .await
                .expect("connect Redis recovery probe");
            let _: () = redis.flushdb().await.expect("clear scoreboard test Redis");
            let rebuilt_after_clear = cached_service
                .admin(contest_id, &admin, query())
                .await
                .expect("rebuild cleared scoreboard cache from PostgreSQL");
            assert_ne!(rebuilt_after_clear.generated_at, first.generated_at);
            assert_eq!(rebuilt_after_clear.rows.len(), first.rows.len());

            sqlx::query("UPDATE teams SET name = 'Renamed Official Team' WHERE id = $1")
                .bind(teams[0])
                .execute(&pool)
                .await
                .expect("rename team and bump scoreboard revision");
            let rebuilt = cached_service
                .admin(contest_id, &admin, query())
                .await
                .expect("bypass stale revision cache");
            assert_ne!(rebuilt.generated_at, first.generated_at);
            assert_eq!(
                rebuilt
                    .rows
                    .iter()
                    .find(|row| row.team_id == teams[0])
                    .expect("renamed team remains visible")
                    .team_name,
                "Renamed Official Team"
            );
        }
    }

    #[test]
    fn icpc_frozen_score_uses_the_assigned_problem_maximum() {
        let start = OffsetDateTime::UNIX_EPOCH;
        let cells = score_submissions(
            start,
            "ICPC",
            "BEST",
            vec![SubmissionScoreRow {
                submission_id: 1,
                team_id: 7,
                problem_id: 10,
                submitted_at: start + Duration::minutes(5),
                verdict: "ACCEPTED".to_owned(),
                score_milli: 100_000,
                max_score_milli: 250_000,
            }],
        );
        assert_eq!(cells[0].score_milli, 250_000);
    }

    #[test]
    fn same_second_ties_use_team_id_for_rank_and_first_blood() {
        let solved_at =
            OffsetDateTime::from_unix_timestamp(1_750_000_000).expect("fixed scoreboard timestamp");
        let board = super::assemble(
            1,
            "ADMIN",
            false,
            solved_at,
            "ICPC".to_owned(),
            "BEST".to_owned(),
            vec![super::ScoreboardProblem {
                problem_id: 10,
                alias: "A".to_owned(),
                display_order: 1,
                first_blood_team_id: None,
                first_blood_at: None,
            }],
            vec![
                super::RosterRow {
                    team_id: 20,
                    team_name: "Higher ID".to_owned(),
                    school: None,
                    participation_type: "OFFICIAL".to_owned(),
                    group_name: None,
                    team_star: false,
                },
                super::RosterRow {
                    team_id: 10,
                    team_name: "Lower ID".to_owned(),
                    school: None,
                    participation_type: "OFFICIAL".to_owned(),
                    group_name: None,
                    team_star: false,
                },
            ],
            vec![
                super::CellRow {
                    team_id: 20,
                    problem_id: 10,
                    wrong_attempts: 0,
                    solved: true,
                    solved_at: Some(solved_at),
                    penalty_minutes: 60,
                    score_milli: 100_000,
                },
                super::CellRow {
                    team_id: 10,
                    problem_id: 10,
                    wrong_attempts: 0,
                    solved: true,
                    solved_at: Some(solved_at),
                    penalty_minutes: 60,
                    score_milli: 100_000,
                },
            ],
        );
        assert_eq!(board.rows[0].team_id, 10);
        assert_eq!(board.rows[0].rank, 1);
        assert!(board.rows[0].problems[0].first_blood);
        assert_eq!(board.problems[0].first_blood_team_id, Some(10));
    }

    async fn insert_scored_submission(
        pool: &PgPool,
        contest_id: i64,
        problem_id: i64,
        team_id: i64,
        submitted_at: OffsetDateTime,
        verdict: &str,
    ) {
        let submission_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, source_sha256, status, submitted_at, judged_at)
            VALUES ($1, $2, $3, 'cpp', $4, 10, $5, $6, $7, now())
            RETURNING id
            "#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .bind(team_id)
        .bind(format!("scoreboard/{team_id}/{}.cpp", Uuid::new_v4()))
        .bind("c".repeat(64))
        .bind(verdict)
        .bind(submitted_at)
        .fetch_one(pool)
        .await
        .expect("insert scored submission");
        sqlx::query(
            r#"
            INSERT INTO judgements (id, submission_id, verdict, completed_at)
            VALUES ($1, $2, $3, now())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(submission_id)
        .bind(verdict)
        .execute(pool)
        .await
        .expect("insert scored judgement");
    }
}
