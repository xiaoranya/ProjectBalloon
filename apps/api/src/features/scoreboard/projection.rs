use sqlx::{Postgres, Transaction};

pub(crate) async fn rebuild_cell(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO contest_scoreboard_cells (
            contest_id, team_id, problem_id, wrong_attempts, solved, solved_at,
            first_accepted_submission_id, penalty_minutes, updated_at
        )
        SELECT
            $1,
            $2,
            $3,
            (
                SELECT count(*)::integer
                FROM submissions rejected
                JOIN judgements rejected_judgement
                  ON rejected_judgement.submission_id = rejected.id
                 AND rejected_judgement.active_marker IS TRUE
                WHERE rejected.contest_id = $1
                  AND rejected.team_id = $2
                  AND rejected.problem_id = $3
                  AND rejected_judgement.verdict IN (
                      'WRONG_ANSWER', 'TIME_LIMIT_EXCEEDED', 'MEMORY_LIMIT_EXCEEDED',
                      'RUNTIME_ERROR', 'OUTPUT_LIMIT_EXCEEDED'
                  )
                  AND (
                      accepted.id IS NULL
                      OR (rejected.submitted_at, rejected.id)
                         < (accepted.submitted_at, accepted.id)
                  )
            ),
            accepted.id IS NOT NULL,
            accepted.submitted_at,
            accepted.id,
            CASE
                WHEN accepted.id IS NULL THEN 0
                ELSE floor(extract(epoch FROM (accepted.submitted_at - contest.start_at)) / 60)::bigint
                     + 20 * (
                         SELECT count(*)
                         FROM submissions rejected
                         JOIN judgements rejected_judgement
                           ON rejected_judgement.submission_id = rejected.id
                          AND rejected_judgement.active_marker IS TRUE
                         WHERE rejected.contest_id = $1
                           AND rejected.team_id = $2
                           AND rejected.problem_id = $3
                           AND rejected_judgement.verdict IN (
                               'WRONG_ANSWER', 'TIME_LIMIT_EXCEEDED', 'MEMORY_LIMIT_EXCEEDED',
                               'RUNTIME_ERROR', 'OUTPUT_LIMIT_EXCEEDED'
                           )
                           AND (rejected.submitted_at, rejected.id)
                               < (accepted.submitted_at, accepted.id)
                     )
            END,
            now()
        FROM contests contest
        LEFT JOIN LATERAL (
            SELECT accepted_submission.id, accepted_submission.submitted_at
            FROM submissions accepted_submission
            JOIN judgements accepted_judgement
              ON accepted_judgement.submission_id = accepted_submission.id
             AND accepted_judgement.active_marker IS TRUE
            WHERE accepted_submission.contest_id = $1
              AND accepted_submission.team_id = $2
              AND accepted_submission.problem_id = $3
              AND accepted_judgement.verdict = 'ACCEPTED'
            ORDER BY accepted_submission.submitted_at, accepted_submission.id
            LIMIT 1
        ) accepted ON true
        WHERE contest.id = $1
          AND contest.start_at IS NOT NULL
        ON CONFLICT (contest_id, team_id, problem_id) DO UPDATE SET
            wrong_attempts = EXCLUDED.wrong_attempts,
            solved = EXCLUDED.solved,
            solved_at = EXCLUDED.solved_at,
            first_accepted_submission_id = EXCLUDED.first_accepted_submission_id,
            penalty_minutes = EXCLUDED.penalty_minutes,
            updated_at = now()
        "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .bind(problem_id)
    .execute(&mut **transaction)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO contest_scoreboard_rows (
            contest_id, team_id, solved_count, penalty_minutes, last_solved_at, updated_at
        )
        SELECT
            $1,
            $2,
            count(*) FILTER (WHERE solved)::integer,
            coalesce(sum(penalty_minutes) FILTER (WHERE solved), 0)::bigint,
            max(solved_at) FILTER (WHERE solved),
            now()
        FROM contest_scoreboard_cells
        WHERE contest_id = $1 AND team_id = $2
        ON CONFLICT (contest_id, team_id) DO UPDATE SET
            solved_count = EXCLUDED.solved_count,
            penalty_minutes = EXCLUDED.penalty_minutes,
            last_solved_at = EXCLUDED.last_solved_at,
            updated_at = now()
        "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
