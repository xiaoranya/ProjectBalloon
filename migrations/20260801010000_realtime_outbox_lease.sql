ALTER TABLE realtime_outbox
    ADD COLUMN lease_owner uuid,
    ADD CONSTRAINT realtime_outbox_lease_shape CHECK (
        (status = 'PUBLISHING' AND lease_owner IS NOT NULL)
        OR
        (status <> 'PUBLISHING' AND lease_owner IS NULL)
    );

CREATE INDEX idx_realtime_outbox_expired_lease
    ON realtime_outbox (available_at, id)
    WHERE status = 'PUBLISHING';
