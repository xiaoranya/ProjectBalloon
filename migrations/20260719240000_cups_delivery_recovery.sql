ALTER TABLE print_requests
    ADD COLUMN delivery_attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN delivery_lease_owner uuid,
    ADD COLUMN delivery_lease_until timestamptz,
    ADD COLUMN submitted_at timestamptz,
    ADD COLUMN cancellation_pending boolean NOT NULL DEFAULT false,
    ADD COLUMN last_delivery_error varchar(255),
    ADD CONSTRAINT print_request_delivery_lease_shape
        CHECK ((delivery_lease_owner IS NULL) = (delivery_lease_until IS NULL)),
    ADD CONSTRAINT print_request_delivery_attempts_valid CHECK (delivery_attempts >= 0),
    ADD CONSTRAINT print_request_cancellation_shape
        CHECK (NOT cancellation_pending OR (status = 'CANCELLED' AND cups_job_id IS NOT NULL));

CREATE INDEX idx_print_requests_delivery_claim
    ON print_requests (status, delivery_lease_until, created_at, id)
    WHERE status IN ('QUEUED', 'PRINTING') OR cancellation_pending;
