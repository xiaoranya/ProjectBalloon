CREATE TABLE submission_export_tasks (
    id BIGSERIAL PRIMARY KEY,
    contest_id BIGINT NOT NULL REFERENCES contests(id) ON DELETE CASCADE,
    requested_by BIGINT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    kind VARCHAR(32) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'QUEUED',
    output_bucket VARCHAR(255),
    output_object_key TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_submission_export_kind CHECK (kind IN ('METADATA_CSV', 'SOURCES_ZIP')),
    CONSTRAINT ck_submission_export_status CHECK (
        status IN ('QUEUED', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'EXPIRED')
    ),
    CONSTRAINT ck_submission_export_attempts CHECK (attempts >= 0),
    CONSTRAINT ck_submission_export_lease CHECK (
        (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
    ),
    CONSTRAINT ck_submission_export_output CHECK (
        (status = 'SUCCEEDED' AND output_bucket IS NOT NULL AND output_object_key IS NOT NULL)
        OR status <> 'SUCCEEDED'
    )
);

CREATE INDEX idx_submission_export_available
    ON submission_export_tasks (available_at, id)
    WHERE status IN ('QUEUED', 'FAILED');

CREATE INDEX idx_submission_export_expiry
    ON submission_export_tasks (expires_at, id)
    WHERE status = 'SUCCEEDED';

CREATE INDEX idx_submission_export_contest
    ON submission_export_tasks (contest_id, created_at DESC, id DESC);
