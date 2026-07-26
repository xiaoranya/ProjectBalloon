-- Application-level duplicate checks are race-prone. Preserve the legacy
-- ability to reuse a soft-deleted contest name while enforcing active-name
-- uniqueness in PostgreSQL.
CREATE UNIQUE INDEX idx_contests_active_name_unique
    ON contests (name)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_contests_updated_at
    ON contests (updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_contests_start_at
    ON contests (start_at DESC, id DESC)
    WHERE deleted_at IS NULL;
