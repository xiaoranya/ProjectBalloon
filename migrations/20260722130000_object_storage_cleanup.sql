CREATE TABLE object_storage_cleanup_tasks (
    id BIGSERIAL PRIMARY KEY,
    bucket VARCHAR(255) NOT NULL,
    object_key TEXT NOT NULL,
    reason VARCHAR(64) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'PENDING',
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_object_storage_cleanup_object UNIQUE (bucket, object_key),
    CONSTRAINT ck_object_storage_cleanup_bucket_nonempty CHECK (length(btrim(bucket)) > 0),
    CONSTRAINT ck_object_storage_cleanup_key_nonempty CHECK (length(btrim(object_key)) > 0),
    CONSTRAINT ck_object_storage_cleanup_reason_nonempty CHECK (length(btrim(reason)) > 0),
    CONSTRAINT ck_object_storage_cleanup_status CHECK (
        status IN ('PENDING', 'PROCESSING', 'FAILED')
    ),
    CONSTRAINT ck_object_storage_cleanup_attempts CHECK (attempts >= 0),
    CONSTRAINT ck_object_storage_cleanup_lease CHECK (
        (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
    )
);

CREATE INDEX idx_object_storage_cleanup_available
    ON object_storage_cleanup_tasks (available_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_object_storage_cleanup_expired_lease
    ON object_storage_cleanup_tasks (lease_until, id)
    WHERE status = 'PROCESSING';
