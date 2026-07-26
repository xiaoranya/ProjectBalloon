UPDATE announcements SET status = upper(status);
ALTER TABLE announcements ALTER COLUMN status SET DEFAULT 'PUBLISHED';

ALTER TABLE announcements
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT announcement_status_known
        CHECK (status IN ('PUBLISHED', 'WITHDRAWN', 'SCHEDULED', 'CANCELLED')),
    ADD CONSTRAINT announcement_text_bounds
        CHECK (char_length(title) BETWEEN 1 AND 255 AND char_length(body) BETWEEN 1 AND 16000),
    ADD CONSTRAINT announcement_state_shape CHECK (
        (status = 'PUBLISHED' AND published_at IS NOT NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'WITHDRAWN' AND published_at IS NOT NULL
            AND withdrawn_at IS NOT NULL AND withdrawn_by IS NOT NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'SCHEDULED' AND scheduled_at IS NOT NULL AND published_at IS NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'CANCELLED' AND scheduled_at IS NOT NULL AND published_at IS NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NOT NULL AND cancelled_by IS NOT NULL)
    );

CREATE UNIQUE INDEX uq_announcements_source_clarification
    ON announcements (source_clarification_id)
    WHERE source_clarification_id IS NOT NULL;

CREATE INDEX idx_announcements_public_order
    ON announcements (contest_id, pinned DESC, published_at DESC, id DESC)
    WHERE status = 'PUBLISHED';
