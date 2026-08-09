use std::cmp::Ordering;
use std::collections::HashMap;

use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};

use crate::error::AppError;
use crate::features::scoreboard::{ScoreboardResponse, ScoreboardRow};

use super::model::{ResolverState, Reveal};

pub(super) struct SourceSnapshot {
    pub(super) contest_id: i64,
    pub(super) variant: String,
    pub(super) frozen: bool,
    pub(super) sha256: String,
    pub(super) board: ScoreboardResponse,
}

pub(super) async fn load_source_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<SourceSnapshot, AppError> {
    let row = sqlx::query_as::<_, (i64, String, bool, String, Option<String>)>(
        "SELECT contest_id, variant, frozen, payload_json, payload_sha256 FROM scoreboard_snapshots WHERE id = $1",
    ).bind(id).fetch_optional(&mut **tx).await
        .map_err(|error| AppError::internal("load Resolver source snapshot", error))?
        .ok_or_else(|| AppError::not_found("SCOREBOARD_SNAPSHOT_NOT_FOUND", "Source snapshot was not found"))?;
    Ok(SourceSnapshot {
        contest_id: row.0,
        variant: row.1,
        frozen: row.2,
        board: serde_json::from_str(&row.3)
            .map_err(|error| AppError::internal("decode Resolver source snapshot", error))?,
        sha256: row.4.ok_or_else(|| {
            AppError::internal("load Resolver source snapshot", "snapshot has no SHA-256")
        })?,
    })
}

pub(super) fn build_states(
    mut current: ScoreboardResponse,
    final_board: ScoreboardResponse,
) -> Result<Vec<ResolverState>, AppError> {
    if current.contest_id != final_board.contest_id {
        return Err(AppError::validation("snapshots", "must describe the same contest"));
    }
    let final_rows =
        final_board.rows.iter().map(|row| (row.team_id, row)).collect::<HashMap<_, _>>();
    if current.rows.len() != final_rows.len()
        || current.problems.len() != final_board.problems.len()
        || current.problems.iter().any(|problem| {
            !final_board.problems.iter().any(|candidate| candidate.problem_id == problem.problem_id)
        })
    {
        return Err(AppError::validation("snapshots", "team and problem sets must match exactly"));
    }
    let mut pending = Vec::new();
    for row in &current.rows {
        let final_row = final_rows
            .get(&row.team_id)
            .ok_or_else(|| AppError::validation("snapshots", "team sets do not match"))?;
        for cell in &row.problems {
            let final_cell = final_row
                .problems
                .iter()
                .find(|candidate| candidate.problem_id == cell.problem_id)
                .ok_or_else(|| AppError::validation("snapshots", "problem sets do not match"))?;
            if cell != final_cell {
                pending.push((row.team_id, cell.problem_id));
            }
        }
    }
    let total_steps = i32::try_from(pending.len())
        .map_err(|error| AppError::internal("count resolver plan", error))?;
    let mut states = vec![ResolverState {
        step_index: 0,
        total_steps,
        board: current.clone(),
        last_reveal: None,
    }];
    while !pending.is_empty() {
        pending.sort_by_key(|(team_id, problem_id)| {
            let rank =
                current.rows.iter().find(|row| row.team_id == *team_id).map_or(0, |row| row.rank);
            (std::cmp::Reverse(rank), *team_id, *problem_id)
        });
        let (team_id, problem_id) = pending.remove(0);
        let row =
            current.rows.iter_mut().find(|row| row.team_id == team_id).ok_or_else(|| {
                AppError::internal("build resolver plan", "current team disappeared")
            })?;
        let final_row = final_rows[&team_id];
        let cell = row.problems.iter_mut().find(|cell| cell.problem_id == problem_id).ok_or_else(
            || AppError::internal("build resolver plan", "current problem disappeared"),
        )?;
        let final_cell =
            final_row.problems.iter().find(|cell| cell.problem_id == problem_id).ok_or_else(
                || AppError::internal("build resolver plan", "final problem disappeared"),
            )?;
        let before = cell.clone();
        *cell = final_cell.clone();
        let reveal = Reveal { team_id, problem_id, before, after: final_cell.clone() };
        recompute_board(&mut current);
        let step_index = i32::try_from(states.len())
            .map_err(|error| AppError::internal("convert resolver plan step", error))?;
        states.push(ResolverState {
            step_index,
            total_steps,
            board: current.clone(),
            last_reveal: Some(reveal),
        });
    }
    Ok(states)
}

fn recompute_board(board: &mut ScoreboardResponse) {
    for row in &mut board.rows {
        row.solved_count = i32::try_from(row.problems.iter().filter(|cell| cell.solved).count())
            .unwrap_or(i32::MAX);
        row.penalty_minutes =
            row.problems.iter().filter(|cell| cell.solved).map(|cell| cell.penalty_minutes).sum();
        row.last_solved_at = row.problems.iter().filter_map(|cell| cell.solved_at).max();
        for cell in &mut row.problems {
            cell.first_blood = false;
        }
    }
    board.rows.sort_by(compare_rows);
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
    for problem in &mut board.problems {
        let first = board
            .rows
            .iter()
            .filter(|row| row.participation_type != "PRACTICE")
            .filter_map(|row| {
                row.problems
                    .iter()
                    .find(|cell| cell.problem_id == problem.problem_id && cell.solved)
                    .and_then(|cell| cell.solved_at.map(|at| (at, row.team_id)))
            })
            .min();
        problem.first_blood_at = first.map(|value| value.0);
        problem.first_blood_team_id = first.map(|value| value.1);
        if let Some((_, team_id)) = first
            && let Some(cell) =
                board.rows.iter_mut().find(|row| row.team_id == team_id).and_then(|row| {
                    row.problems.iter_mut().find(|cell| cell.problem_id == problem.problem_id)
                })
        {
            cell.first_blood = true;
        }
    }
}

fn compare_rows(left: &ScoreboardRow, right: &ScoreboardRow) -> Ordering {
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

pub(super) fn encode_state(state: &ResolverState) -> Result<(String, String), AppError> {
    let encoded = serde_json::to_string(state)
        .map_err(|error| AppError::internal("encode resolver state", error))?;
    let sha = hex::encode(Sha256::digest(encoded.as_bytes()));
    Ok((encoded, sha))
}
