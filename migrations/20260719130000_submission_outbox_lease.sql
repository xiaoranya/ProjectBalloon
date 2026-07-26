ALTER TABLE submission_outbox
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN lease_owner uuid,
    ADD COLUMN lease_until timestamptz,
    ADD CONSTRAINT submission_outbox_lease_shape CHECK (
        (status = 'PUBLISHING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PUBLISHING' AND lease_owner IS NULL AND lease_until IS NULL)
    );

DROP INDEX idx_outbox_status_pending;

CREATE INDEX idx_submission_outbox_dispatchable
    ON submission_outbox (available_at, created_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_submission_outbox_expired_lease
    ON submission_outbox (lease_until, id)
    WHERE status = 'PUBLISHING';
