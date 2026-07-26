-- Business transactions persist realtime notifications before commit. A
-- dispatcher introduced with the realtime slice will publish pending rows to
-- Redis/SSE with retry and mark them delivered.
CREATE TABLE realtime_outbox (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id uuid NOT NULL UNIQUE,
    contest_id bigint NOT NULL REFERENCES contests(id),
    event_type varchar(64) NOT NULL,
    schema_version smallint NOT NULL DEFAULT 1,
    scope varchar(16) NOT NULL,
    payload_json jsonb NOT NULL,
    status varchar(16) NOT NULL DEFAULT 'PENDING',
    attempts integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    published_at timestamptz,
    last_error text,
    CONSTRAINT realtime_outbox_scope_check
        CHECK (scope IN ('PUBLIC', 'STAFF', 'TEAM')),
    CONSTRAINT realtime_outbox_schema_version_check
        CHECK (schema_version > 0),
    CONSTRAINT realtime_outbox_status_check
        CHECK (status IN ('PENDING', 'PUBLISHING', 'PUBLISHED', 'FAILED')),
    CONSTRAINT realtime_outbox_attempts_check
        CHECK (attempts >= 0)
);

CREATE INDEX idx_realtime_outbox_pending
    ON realtime_outbox (available_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_realtime_outbox_contest_created
    ON realtime_outbox (contest_id, created_at DESC);
