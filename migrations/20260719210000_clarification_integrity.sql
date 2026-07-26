UPDATE clarifications SET status = upper(status);
ALTER TABLE clarifications ALTER COLUMN status SET DEFAULT 'PENDING';

ALTER TABLE clarifications
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD COLUMN closed_by bigint REFERENCES users(id),
    ADD COLUMN closed_at timestamptz,
    ADD CONSTRAINT clarification_scope_known CHECK (scope IN ('GENERAL', 'PROBLEM')),
    ADD CONSTRAINT clarification_scope_problem_shape
        CHECK ((scope = 'GENERAL' AND problem_id IS NULL AND problem_alias IS NULL)
            OR (scope = 'PROBLEM' AND problem_id IS NOT NULL AND problem_alias IS NOT NULL)),
    ADD CONSTRAINT clarification_status_known CHECK (status IN ('PENDING', 'ANSWERED', 'CLOSED')),
    ADD CONSTRAINT clarification_reply_visibility_known
        CHECK (reply_visibility IS NULL OR reply_visibility IN ('PRIVATE', 'PUBLIC')),
    ADD CONSTRAINT clarification_reply_shape
        CHECK ((status = 'PENDING' AND reply IS NULL AND reply_visibility IS NULL
                AND replied_by IS NULL AND replied_at IS NULL)
            OR (status = 'ANSWERED' AND reply IS NOT NULL AND reply_visibility IS NOT NULL
                AND replied_by IS NOT NULL AND replied_at IS NOT NULL)
            OR status = 'CLOSED'),
    ADD CONSTRAINT clarification_closed_shape
        CHECK ((status = 'CLOSED' AND closed_by IS NOT NULL AND closed_at IS NOT NULL)
            OR (status <> 'CLOSED' AND closed_by IS NULL AND closed_at IS NULL)),
    ADD CONSTRAINT clarification_text_bounds
        CHECK (char_length(question) BETWEEN 1 AND 4000
            AND (reply IS NULL OR char_length(reply) BETWEEN 1 AND 8000));

CREATE INDEX idx_clarifications_rate_limit
    ON clarifications (contest_id, team_id, created_at DESC);
