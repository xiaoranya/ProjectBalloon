CREATE TABLE object_storage_integrity_findings (
    id BIGSERIAL PRIMARY KEY,
    bucket VARCHAR(255) NOT NULL,
    object_key TEXT NOT NULL,
    first_detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    CONSTRAINT uq_object_storage_integrity_object UNIQUE (bucket, object_key),
    CONSTRAINT ck_object_storage_integrity_bucket_nonempty CHECK (length(btrim(bucket)) > 0),
    CONSTRAINT ck_object_storage_integrity_key_nonempty CHECK (length(btrim(object_key)) > 0),
    CONSTRAINT ck_object_storage_integrity_timestamps CHECK (
        last_detected_at >= first_detected_at
        AND (resolved_at IS NULL OR resolved_at >= first_detected_at)
    )
);

CREATE INDEX idx_object_storage_integrity_unresolved
    ON object_storage_integrity_findings (last_detected_at, id)
    WHERE resolved_at IS NULL;
