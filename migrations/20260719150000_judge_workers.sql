CREATE TABLE judge_workers (
    worker_id varchar(64) PRIMARY KEY,
    instance_id uuid NOT NULL,
    started_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    capacity smallint NOT NULL,
    active_tasks smallint NOT NULL,
    languages jsonb NOT NULL,
    runtime_versions jsonb NOT NULL,
    sandbox_runtime varchar(64),
    last_message_id uuid NOT NULL UNIQUE,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT judge_workers_capacity_check
        CHECK (capacity > 0 AND active_tasks BETWEEN 0 AND capacity),
    CONSTRAINT judge_workers_languages_array_check
        CHECK (jsonb_typeof(languages) = 'array'),
    CONSTRAINT judge_workers_runtime_versions_object_check
        CHECK (jsonb_typeof(runtime_versions) = 'object')
);

CREATE INDEX idx_judge_workers_last_seen
    ON judge_workers (last_seen_at DESC, worker_id);
