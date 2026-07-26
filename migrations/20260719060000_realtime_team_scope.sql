-- TEAM events must name their private recipient. PUBLIC and STAFF events must
-- not carry a team identifier, which prevents accidental cross-scope fanout.
ALTER TABLE realtime_outbox
    ADD COLUMN team_id bigint REFERENCES teams(id);

ALTER TABLE realtime_outbox
    ADD CONSTRAINT realtime_outbox_recipient_check
    CHECK (
        (scope = 'TEAM' AND team_id IS NOT NULL)
        OR (scope IN ('PUBLIC', 'STAFF') AND team_id IS NULL)
    );
