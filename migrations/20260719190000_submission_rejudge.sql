ALTER TABLE submission_outbox
    DROP CONSTRAINT submission_outbox_status_known,
    ADD CONSTRAINT submission_outbox_status_known
        CHECK (status IN ('PENDING', 'PUBLISHING', 'SENT', 'FAILED', 'CANCELLED'));

COMMENT ON COLUMN submission_outbox.status IS
    'CANCELLED is terminal for a task superseded before or during publication by a rejudge.';
