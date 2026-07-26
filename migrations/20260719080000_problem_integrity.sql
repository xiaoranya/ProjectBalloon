ALTER TABLE problems DROP CONSTRAINT problems_slug_key;
DROP INDEX idx_problems_slug_alive;

CREATE UNIQUE INDEX idx_problems_active_slug_unique
    ON problems (slug)
    WHERE deleted_at IS NULL;

ALTER TABLE problems
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT problems_time_limit_positive CHECK (time_limit_ms > 0),
    ADD CONSTRAINT problems_memory_limit_positive CHECK (memory_limit_mb > 0),
    ADD CONSTRAINT problems_output_limit_positive CHECK (output_limit_kb > 0),
    ADD CONSTRAINT problems_testdata_version_nonnegative CHECK (testdata_version >= 0),
    ADD CONSTRAINT problems_version_nonnegative CHECK (version >= 0);
