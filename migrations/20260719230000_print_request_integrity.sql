UPDATE print_requests SET failed_reason = NULL
WHERE status NOT IN ('FAILED', 'REJECTED');
UPDATE print_requests SET completed_at = NULL
WHERE status <> 'COMPLETED';

ALTER TABLE print_requests
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT print_request_status_known
        CHECK (status IN ('REQUESTED', 'QUEUED', 'PRINTING', 'COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED')),
    ADD CONSTRAINT print_request_content_bounds
        CHECK (octet_length(content) BETWEEN 1 AND 20480 AND page_count BETWEEN 1 AND 5),
    ADD CONSTRAINT print_request_hash_shape
        CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT print_request_pdf_shape
        CHECK (status = 'REQUESTED' OR (pdf_object_key IS NOT NULL AND pdf_bucket IS NOT NULL)),
    ADD CONSTRAINT print_request_completion_shape
        CHECK ((status = 'COMPLETED' AND completed_at IS NOT NULL)
            OR (status <> 'COMPLETED' AND completed_at IS NULL)),
    ADD CONSTRAINT print_request_failure_shape
        CHECK ((status IN ('FAILED', 'REJECTED') AND failed_reason IS NOT NULL)
            OR (status NOT IN ('FAILED', 'REJECTED') AND failed_reason IS NULL));

CREATE INDEX idx_print_requests_queue_order
    ON print_requests (contest_id, created_at, id)
    WHERE status = 'QUEUED';
