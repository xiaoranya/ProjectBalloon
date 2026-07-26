UPDATE resolver_runs SET status = upper(status);
UPDATE resolver_runs SET status = 'READY'
WHERE status NOT IN ('READY', 'RUNNING', 'PAUSED', 'COMPLETED');
UPDATE resolver_runs SET current_step = greatest(current_step, 0),
    total_steps = greatest(total_steps, current_step, 0);

ALTER TABLE resolver_runs
    ADD COLUMN source_public_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN source_final_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN plan_sha256 char(64) NOT NULL DEFAULT repeat('0', 64),
    ADD COLUMN created_by_user_id bigint REFERENCES users(id),
    ADD COLUMN started_at timestamptz,
    ADD COLUMN completed_at timestamptz,
    ADD COLUMN auto_play_enabled boolean NOT NULL DEFAULT false,
    ADD COLUMN auto_play_interval_ms integer NOT NULL DEFAULT 3000,
    ADD COLUMN next_auto_at timestamptz,
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT resolver_run_status_known
        CHECK (status IN ('READY', 'RUNNING', 'PAUSED', 'COMPLETED')),
    ADD CONSTRAINT resolver_run_step_bounds
        CHECK (current_step >= 0 AND total_steps >= 0 AND current_step <= total_steps),
    ADD CONSTRAINT resolver_run_plan_sha256_shape CHECK (plan_sha256 ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT resolver_run_auto_interval_bounds
        CHECK (auto_play_interval_ms BETWEEN 500 AND 60000),
    ADD CONSTRAINT resolver_run_auto_shape CHECK (
        (auto_play_enabled AND status = 'RUNNING' AND next_auto_at IS NOT NULL
            AND current_step < total_steps)
        OR (NOT auto_play_enabled AND next_auto_at IS NULL)
    ),
    ADD CONSTRAINT resolver_run_source_pair CHECK (
        (source_public_snapshot_id IS NULL) = (source_final_snapshot_id IS NULL)
    );

UPDATE resolver_runs SET started_at = NULL, completed_at = NULL WHERE status = 'READY';
UPDATE resolver_runs SET started_at = coalesce(started_at, created_at), completed_at = NULL
WHERE status IN ('RUNNING', 'PAUSED');
UPDATE resolver_runs SET current_step = total_steps,
    started_at = coalesce(started_at, created_at), completed_at = coalesce(completed_at, updated_at)
WHERE status = 'COMPLETED';

ALTER TABLE resolver_runs
    ADD CONSTRAINT resolver_run_time_shape CHECK (
        (status = 'READY' AND started_at IS NULL AND completed_at IS NULL)
        OR (status IN ('RUNNING', 'PAUSED') AND started_at IS NOT NULL AND completed_at IS NULL)
        OR (status = 'COMPLETED' AND started_at IS NOT NULL AND completed_at IS NOT NULL
            AND current_step = total_steps)
    );

CREATE UNIQUE INDEX uq_resolver_official_run
    ON resolver_runs (contest_id) WHERE official;

CREATE INDEX idx_resolver_auto_due
    ON resolver_runs (next_auto_at, id)
    WHERE auto_play_enabled AND status = 'RUNNING';

ALTER TABLE resolver_snapshots
    ADD COLUMN state_sha256 char(64),
    ADD CONSTRAINT resolver_snapshot_step_valid CHECK (step_index >= 0),
    ADD CONSTRAINT resolver_snapshot_sha256_shape
        CHECK (state_sha256 IS NULL OR state_sha256 ~ '^[0-9a-f]{64}$');

CREATE UNIQUE INDEX uq_resolver_snapshot_step
    ON resolver_snapshots (run_id, step_index);

ALTER TABLE resolver_current_state
    ADD COLUMN state_sha256 char(64),
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT resolver_current_step_valid CHECK (step_index >= 0),
    ADD CONSTRAINT resolver_current_sha256_shape
        CHECK (state_sha256 IS NULL OR state_sha256 ~ '^[0-9a-f]{64}$');

ALTER TABLE resolver_events
    ADD COLUMN actor_user_id bigint REFERENCES users(id),
    ADD CONSTRAINT resolver_event_sequence_valid CHECK (sequence >= 0);

CREATE UNIQUE INDEX uq_resolver_event_sequence ON resolver_events (run_id, sequence);

CREATE TABLE resolver_pending_submissions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL REFERENCES resolver_runs(id),
    submission_id bigint NOT NULL REFERENCES submissions(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    problem_id bigint NOT NULL REFERENCES problems(id),
    submitted_at timestamptz NOT NULL,
    verdict_at_snapshot varchar(32) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (run_id, submission_id)
);

CREATE INDEX idx_resolver_pending_run_order
    ON resolver_pending_submissions (run_id, team_id, problem_id, submitted_at, submission_id);

CREATE OR REPLACE FUNCTION reject_resolver_history_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'resolver history is immutable';
END;
$$;

CREATE TRIGGER trg_resolver_snapshots_immutable
BEFORE UPDATE OR DELETE ON resolver_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE TRIGGER trg_resolver_events_immutable
BEFORE UPDATE OR DELETE ON resolver_events
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE TRIGGER trg_resolver_pending_immutable
BEFORE UPDATE OR DELETE ON resolver_pending_submissions
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE OR REPLACE FUNCTION protect_resolver_run_sources()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.contest_id IS DISTINCT FROM OLD.contest_id
       OR NEW.official IS DISTINCT FROM OLD.official
       OR NEW.source_public_snapshot_id IS DISTINCT FROM OLD.source_public_snapshot_id
       OR NEW.source_final_snapshot_id IS DISTINCT FROM OLD.source_final_snapshot_id
       OR NEW.plan_sha256 IS DISTINCT FROM OLD.plan_sha256
       OR NEW.total_steps IS DISTINCT FROM OLD.total_steps
       OR NEW.created_by_user_id IS DISTINCT FROM OLD.created_by_user_id THEN
        RAISE EXCEPTION 'resolver run sources are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_resolver_run_sources_immutable
BEFORE UPDATE ON resolver_runs
FOR EACH ROW EXECUTE FUNCTION protect_resolver_run_sources();
