ALTER TABLE contests
    ADD COLUMN scoring_mode varchar(16) NOT NULL DEFAULT 'ICPC',
    ADD COLUMN score_aggregation varchar(16) NOT NULL DEFAULT 'BEST',
    ADD COLUMN feedback_policy varchar(16) NOT NULL DEFAULT 'FULL',
    ADD CONSTRAINT contests_scoring_mode_known
        CHECK (scoring_mode IN ('ICPC', 'OI', 'IOI')),
    ADD CONSTRAINT contests_score_aggregation_known
        CHECK (score_aggregation IN ('BEST', 'LAST')),
    ADD CONSTRAINT contests_feedback_policy_known
        CHECK (feedback_policy IN ('FULL', 'SCORE_ONLY', 'NONE'));

ALTER TABLE contest_problems
    ADD COLUMN max_score_milli integer NOT NULL DEFAULT 100000,
    ADD CONSTRAINT contest_problems_max_score_valid
        CHECK (max_score_milli BETWEEN 1 AND 100000000);

CREATE TABLE contest_problem_subtasks (
    id bigserial PRIMARY KEY,
    contest_id bigint NOT NULL,
    problem_id bigint NOT NULL,
    subtask_key varchar(32) NOT NULL,
    name varchar(120) NOT NULL,
    display_order integer NOT NULL,
    score_milli integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contest_problem_subtasks_assignment_fk
        FOREIGN KEY (contest_id, problem_id)
        REFERENCES contest_problems(contest_id, problem_id) ON DELETE CASCADE,
    CONSTRAINT contest_problem_subtasks_key_shape
        CHECK (subtask_key ~ '^[A-Z0-9_]{1,32}$'),
    CONSTRAINT contest_problem_subtasks_name_bounds
        CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    CONSTRAINT contest_problem_subtasks_order_valid
        CHECK (display_order BETWEEN 1 AND 1000),
    CONSTRAINT contest_problem_subtasks_score_valid
        CHECK (score_milli BETWEEN 1 AND 100000000),
    CONSTRAINT contest_problem_subtasks_key_unique
        UNIQUE (contest_id, problem_id, subtask_key),
    CONSTRAINT contest_problem_subtasks_order_unique
        UNIQUE (contest_id, problem_id, display_order)
);

CREATE TABLE contest_problem_subtask_tests (
    subtask_id bigint NOT NULL REFERENCES contest_problem_subtasks(id) ON DELETE CASCADE,
    test_index integer NOT NULL CHECK (test_index BETWEEN 1 AND 10000),
    PRIMARY KEY (subtask_id, test_index)
);

CREATE INDEX idx_contest_problem_subtasks_assignment
    ON contest_problem_subtasks(contest_id, problem_id, display_order);

ALTER TABLE judgements
    ADD COLUMN score_milli integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT judgements_score_valid
        CHECK (score_milli BETWEEN 0 AND 100000000);

CREATE TABLE judgement_subtask_scores (
    judgement_id uuid NOT NULL REFERENCES judgements(id) ON DELETE CASCADE,
    subtask_id bigint NOT NULL REFERENCES contest_problem_subtasks(id),
    score_milli integer NOT NULL,
    passed_tests integer NOT NULL,
    total_tests integer NOT NULL,
    PRIMARY KEY (judgement_id, subtask_id),
    CONSTRAINT judgement_subtask_score_valid CHECK (score_milli >= 0),
    CONSTRAINT judgement_subtask_counts_valid
        CHECK (total_tests > 0 AND passed_tests BETWEEN 0 AND total_tests)
);

ALTER TABLE contest_scoreboard_cells
    ADD COLUMN score_milli integer NOT NULL DEFAULT 0,
    ADD COLUMN effective_submission_id bigint REFERENCES submissions(id),
    ADD CONSTRAINT scoreboard_cell_score_valid CHECK (score_milli >= 0);

ALTER TABLE contest_scoreboard_rows
    ADD COLUMN total_score_milli bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT scoreboard_row_score_valid CHECK (total_score_milli >= 0);

CREATE INDEX idx_judgements_submission_score
    ON judgements(submission_id, score_milli DESC)
    WHERE active_marker IS TRUE AND completed_at IS NOT NULL;
