CREATE TABLE competition_workstations (
    id bigserial PRIMARY KEY,
    ip_address varchar(45) NOT NULL UNIQUE,
    seat_no varchar(64) NOT NULL UNIQUE,
    label varchar(128),
    enabled boolean NOT NULL DEFAULT true,
    last_seen_at timestamptz,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT competition_workstations_ip_not_blank CHECK (btrim(ip_address) <> ''),
    CONSTRAINT competition_workstations_seat_not_blank CHECK (btrim(seat_no) <> ''),
    CONSTRAINT competition_workstations_label_not_blank
        CHECK (label IS NULL OR btrim(label) <> '')
);

CREATE TABLE contest_workstation_bindings (
    id bigserial PRIMARY KEY,
    contest_id bigint NOT NULL REFERENCES contests(id) ON DELETE CASCADE,
    workstation_id bigint NOT NULL REFERENCES competition_workstations(id) ON DELETE CASCADE,
    team_id bigint NOT NULL,
    pairing_code_hash character(64) NOT NULL,
    bound_by_user_id bigint NOT NULL REFERENCES users(id),
    bound_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contest_workstation_bindings_roster_fkey
        FOREIGN KEY (contest_id, team_id)
        REFERENCES contest_teams(contest_id, team_id)
        ON DELETE CASCADE,
    CONSTRAINT contest_workstation_bindings_pairing_hash
        CHECK (pairing_code_hash ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX idx_contest_workstation_active_terminal
    ON contest_workstation_bindings(contest_id, workstation_id)
    WHERE revoked_at IS NULL;

CREATE UNIQUE INDEX idx_contest_workstation_active_team
    ON contest_workstation_bindings(contest_id, team_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_contest_workstation_active_lookup
    ON contest_workstation_bindings(workstation_id, contest_id)
    WHERE revoked_at IS NULL;

ALTER TABLE auth_sessions
    ADD COLUMN workstation_binding_id bigint
        REFERENCES contest_workstation_bindings(id) ON DELETE CASCADE,
    ADD COLUMN bound_ip varchar(45),
    ADD CONSTRAINT auth_sessions_workstation_shape CHECK (
        (workstation_binding_id IS NULL AND bound_ip IS NULL)
        OR (workstation_binding_id IS NOT NULL AND bound_ip IS NOT NULL)
    );

CREATE INDEX idx_auth_sessions_workstation_binding
    ON auth_sessions(workstation_binding_id)
    WHERE workstation_binding_id IS NOT NULL;
