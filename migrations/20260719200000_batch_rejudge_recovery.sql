ALTER TABLE batch_rejudge_tasks
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT batch_rejudge_task_status_known
        CHECK (status IN ('PENDING', 'RUNNING', 'PAUSED', 'COMPLETED', 'CANCELLED')),
    ADD CONSTRAINT batch_rejudge_task_counts_valid
        CHECK (
            total_items >= 0 AND processed_items >= 0 AND succeeded_items >= 0 AND failed_items >= 0
            AND processed_items = succeeded_items + failed_items
            AND processed_items <= total_items
        );

ALTER TABLE batch_rejudge_items
    ADD COLUMN attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN lease_owner uuid,
    ADD COLUMN lease_until timestamptz,
    ADD CONSTRAINT batch_rejudge_item_status_known
        CHECK (status IN ('PENDING', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'CANCELLED')),
    ADD CONSTRAINT batch_rejudge_item_lease_shape
        CHECK (
            (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
            OR
            (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
        );

ALTER TABLE judgements
    ADD COLUMN batch_rejudge_item_id bigint REFERENCES batch_rejudge_items(id);

CREATE UNIQUE INDEX uq_judgements_batch_rejudge_item
    ON judgements (batch_rejudge_item_id)
    WHERE batch_rejudge_item_id IS NOT NULL;

CREATE INDEX idx_batch_rejudge_items_claim
    ON batch_rejudge_items (task_id, created_at, id)
    WHERE status IN ('PENDING', 'PROCESSING');
