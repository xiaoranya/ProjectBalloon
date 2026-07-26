ALTER TABLE submissions
    ADD COLUMN source_sha256 varchar(64),
    ADD CONSTRAINT submissions_source_size_bounds
        CHECK (source_size_bytes BETWEEN 1 AND 65536),
    ADD CONSTRAINT submissions_source_sha256_format
        CHECK (source_sha256 IS NULL OR source_sha256 ~ '^[0-9a-f]{64}$');

ALTER TABLE submission_outbox
    ADD CONSTRAINT submission_outbox_attempts_nonnegative CHECK (attempts >= 0),
    ADD CONSTRAINT submission_outbox_status_known
        CHECK (status IN ('PENDING', 'PUBLISHING', 'SENT', 'FAILED'));

CREATE INDEX idx_submissions_team_recent
    ON submissions (team_id, submitted_at DESC, id DESC);

COMMENT ON COLUMN submissions.source_sha256 IS
    'SHA-256 of the exact source bytes dispatched to Judge; NULL only for bridged legacy rows.';
