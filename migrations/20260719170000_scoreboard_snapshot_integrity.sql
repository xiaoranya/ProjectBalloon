ALTER TABLE scoreboard_snapshots
    ADD COLUMN participation_type varchar(32),
    ADD COLUMN payload_sha256 char(64),
    ADD COLUMN created_by_user_id bigint REFERENCES users(id);

ALTER TABLE scoreboard_snapshots
    ADD CONSTRAINT scoreboard_snapshot_participation_type_check
        CHECK (participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')),
    ADD CONSTRAINT scoreboard_snapshot_sha256_check
        CHECK (payload_sha256 IS NULL OR payload_sha256 ~ '^[0-9a-f]{64}$');

CREATE UNIQUE INDEX uq_scoreboard_snapshot_version
    ON scoreboard_snapshots (
        contest_id,
        variant,
        coalesce(group_name, ''),
        coalesce(participation_type, ''),
        version
    );

CREATE OR REPLACE FUNCTION reject_scoreboard_snapshot_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'scoreboard snapshots are immutable';
END;
$$;

CREATE TRIGGER trg_scoreboard_snapshots_immutable
BEFORE UPDATE OR DELETE ON scoreboard_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_scoreboard_snapshot_mutation();
