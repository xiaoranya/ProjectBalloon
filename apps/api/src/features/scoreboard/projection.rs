use sqlx::{Postgres, Transaction};

pub(crate) async fn rebuild_cell(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
) -> Result<(), sqlx::Error> {
    let scoring_mode =
        sqlx::query_scalar::<_, String>("SELECT scoring_mode FROM contests WHERE id=$1")
            .bind(contest_id)
            .fetch_one(&mut **transaction)
            .await?;
    if scoring_mode != "ICPC" {
        return rebuild_points_cell(transaction, contest_id, team_id, problem_id).await;
    }
    sqlx::query(
        r#"
        INSERT INTO contest_scoreboard_cells (
            contest_id, team_id, problem_id, wrong_attempts, solved, solved_at,
            first_accepted_submission_id, penalty_minutes, score_milli,
            effective_submission_id, updated_at
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
            CASE WHEN accepted.id IS NULL THEN 0 ELSE assignment.max_score_milli END,
            accepted.id,
            now()
        FROM contests contest
        JOIN contest_problems assignment
          ON assignment.contest_id = contest.id AND assignment.problem_id = $3
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
            score_milli = EXCLUDED.score_milli,
            effective_submission_id = EXCLUDED.effective_submission_id,
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
            contest_id, team_id, solved_count, penalty_minutes, last_solved_at,
            total_score_milli, updated_at
        )
        SELECT
            $1,
            $2,
            count(*) FILTER (WHERE solved)::integer,
            coalesce(sum(penalty_minutes) FILTER (WHERE solved), 0)::bigint,
            max(solved_at) FILTER (WHERE solved),
            coalesce(sum(score_milli), 0)::bigint,
            now()
        FROM contest_scoreboard_cells
        WHERE contest_id = $1 AND team_id = $2
        ON CONFLICT (contest_id, team_id) DO UPDATE SET
            solved_count = EXCLUDED.solved_count,
            penalty_minutes = EXCLUDED.penalty_minutes,
            last_solved_at = EXCLUDED.last_solved_at,
            total_score_milli = EXCLUDED.total_score_milli,
            updated_at = now()
        "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn rebuild_points_cell(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO contest_scoreboard_cells (
            contest_id, team_id, problem_id, wrong_attempts, solved, solved_at,
            first_accepted_submission_id, penalty_minutes, score_milli,
            effective_submission_id, updated_at
        )
        SELECT $1,$2,$3,
               coalesce(attempts.count,0)::integer,
               coalesce(effective.score_milli,0) >= assignment.max_score_milli,
               CASE WHEN coalesce(effective.score_milli,0) >= assignment.max_score_milli
                    THEN effective.submitted_at END,
               CASE WHEN coalesce(effective.score_milli,0) >= assignment.max_score_milli
                    THEN effective.id END,
               0,
               coalesce(effective.score_milli,0),
               effective.id,
               now()
        FROM contests contest
        JOIN contest_problems assignment
          ON assignment.contest_id=contest.id AND assignment.problem_id=$3
        LEFT JOIN LATERAL (
            SELECT submission.id,submission.submitted_at,judgement.score_milli
            FROM submissions submission
            JOIN judgements judgement
              ON judgement.submission_id=submission.id
             AND judgement.active_marker IS TRUE
             AND judgement.completed_at IS NOT NULL
            WHERE submission.contest_id=$1 AND submission.team_id=$2
              AND submission.problem_id=$3
            ORDER BY
              CASE WHEN contest.score_aggregation='BEST' THEN judgement.score_milli END DESC,
              CASE WHEN contest.score_aggregation='BEST' THEN submission.submitted_at END,
              CASE WHEN contest.score_aggregation='LAST' THEN submission.submitted_at END DESC,
              submission.id DESC
            LIMIT 1
        ) effective ON true
        LEFT JOIN LATERAL (
            SELECT count(*)
            FROM submissions submission
            JOIN judgements judgement
              ON judgement.submission_id=submission.id
             AND judgement.active_marker IS TRUE
             AND judgement.completed_at IS NOT NULL
            WHERE submission.contest_id=$1 AND submission.team_id=$2
              AND submission.problem_id=$3
        ) attempts ON true
        WHERE contest.id=$1
        ON CONFLICT (contest_id,team_id,problem_id) DO UPDATE SET
            wrong_attempts=EXCLUDED.wrong_attempts,
            solved=EXCLUDED.solved,
            solved_at=EXCLUDED.solved_at,
            first_accepted_submission_id=EXCLUDED.first_accepted_submission_id,
            penalty_minutes=0,
            score_milli=EXCLUDED.score_milli,
            effective_submission_id=EXCLUDED.effective_submission_id,
            updated_at=now()
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
            contest_id,team_id,solved_count,penalty_minutes,last_solved_at,
            total_score_milli,updated_at
        )
        SELECT $1,$2,count(*) FILTER (WHERE solved)::integer,0,
               max(solved_at) FILTER (WHERE solved),
               coalesce(sum(score_milli),0)::bigint,now()
        FROM contest_scoreboard_cells
        WHERE contest_id=$1 AND team_id=$2
        ON CONFLICT (contest_id,team_id) DO UPDATE SET
            solved_count=EXCLUDED.solved_count,
            penalty_minutes=0,
            last_solved_at=EXCLUDED.last_solved_at,
            total_score_milli=EXCLUDED.total_score_milli,
            updated_at=now()
        "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
