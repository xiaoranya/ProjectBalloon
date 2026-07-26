-- Team authentication must use an immutable identifier, never a display-name
-- convention. One login belongs to exactly one team and one team has at most
-- one primary contestant login in the first Rust version.
CREATE TABLE team_accounts (
    user_id bigint PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    team_id bigint NOT NULL UNIQUE REFERENCES teams(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE teams
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT teams_version_check CHECK (version >= 0);

CREATE UNIQUE INDEX idx_teams_active_name_unique
    ON teams (lower(name))
    WHERE deleted_at IS NULL;

ALTER TABLE contest_teams
    ADD CONSTRAINT contest_teams_participation_type_check
    CHECK (participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE'));

CREATE INDEX idx_contest_teams_team_id
    ON contest_teams (team_id, contest_id);

CREATE INDEX idx_team_members_team_created
    ON team_members (team_id, created_at, id);
