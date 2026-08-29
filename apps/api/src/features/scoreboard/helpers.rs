use std::{cmp::Ordering, collections::HashMap};

use time::OffsetDateTime;

use crate::error::AppError;

use super::model::{
    CellRow, RosterRow, ScoreboardCell, ScoreboardProblem, ScoreboardResponse, ScoreboardRow,
    SubmissionScoreRow, ValidatedScoreboardQuery,
};

pub(super) fn apply_scoreboard_filter(
    board: &mut ScoreboardResponse,
    query: &ValidatedScoreboardQuery,
) {
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

pub(super) fn score_submissions(
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

pub(super) fn is_penalized_rejection(verdict: &str) -> bool {
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
pub(super) fn assemble(
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

pub(super) fn compare_rows(
    scoring_mode: &str,
    left: &ScoreboardRow,
    right: &ScoreboardRow,
) -> Ordering {
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

pub(super) fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

pub(super) fn to_csv(board: &ScoreboardResponse) -> String {
    let mut output = "rank,officialRank,teamId,teamName,school,participationType,groupName,solvedCount,penaltyMinutes".to_string();
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

pub(super) fn csv_field(value: &str) -> String {
    let safe = if matches!(value.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if safe.chars().any(|character| matches!(character, ',' | '"' | '\r' | '\n')) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}
