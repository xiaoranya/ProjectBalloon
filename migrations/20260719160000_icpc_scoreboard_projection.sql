CREATE TABLE contest_scoreboard_cells (
    contest_id bigint NOT NULL REFERENCES contests(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    problem_id bigint NOT NULL REFERENCES problems(id),
    wrong_attempts integer NOT NULL DEFAULT 0,
    solved boolean NOT NULL DEFAULT false,
    solved_at timestamptz,
    first_accepted_submission_id bigint REFERENCES submissions(id),
    penalty_minutes bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (contest_id, team_id, problem_id),
    CONSTRAINT scoreboard_cell_wrong_attempts_check CHECK (wrong_attempts >= 0),
    CONSTRAINT scoreboard_cell_penalty_check CHECK (penalty_minutes >= 0),
    CONSTRAINT scoreboard_cell_solved_state_check CHECK (
        (solved AND solved_at IS NOT NULL AND first_accepted_submission_id IS NOT NULL)
        OR
        (NOT solved AND solved_at IS NULL AND first_accepted_submission_id IS NULL
            AND penalty_minutes = 0)
    )
);

CREATE TABLE contest_scoreboard_rows (
    contest_id bigint NOT NULL REFERENCES contests(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    solved_count integer NOT NULL DEFAULT 0,
    penalty_minutes bigint NOT NULL DEFAULT 0,
    last_solved_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (contest_id, team_id),
    CONSTRAINT scoreboard_row_solved_count_check CHECK (solved_count >= 0),
    CONSTRAINT scoreboard_row_penalty_check CHECK (penalty_minutes >= 0),
    CONSTRAINT scoreboard_row_last_solve_check CHECK (
        (solved_count = 0 AND last_solved_at IS NULL)
        OR (solved_count > 0 AND last_solved_at IS NOT NULL)
    )
);

CREATE INDEX idx_scoreboard_rows_ranking
    ON contest_scoreboard_rows
        (contest_id, solved_count DESC, penalty_minutes, last_solved_at, team_id);

CREATE INDEX idx_scoreboard_cells_problem_solved
    ON contest_scoreboard_cells (contest_id, problem_id, solved_at, team_id)
    WHERE solved;
