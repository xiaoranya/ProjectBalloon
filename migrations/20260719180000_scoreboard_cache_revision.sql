ALTER TABLE contests
    ADD COLUMN scoreboard_revision bigint NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    affected_contest_id bigint;
BEGIN
    affected_contest_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.contest_id ELSE NEW.contest_id END;
    UPDATE contests
    SET scoreboard_revision = scoreboard_revision + 1
    WHERE id = affected_contest_id;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER trg_scoreboard_cell_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_scoreboard_cells
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE TRIGGER trg_contest_roster_scoreboard_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_teams
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE TRIGGER trg_contest_problem_scoreboard_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_problems
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE OR REPLACE FUNCTION bump_contest_scoreboard_revision_for_team()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.name IS DISTINCT FROM NEW.name
       OR OLD.school IS DISTINCT FROM NEW.school
       OR OLD.star IS DISTINCT FROM NEW.star
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        UPDATE contests contest
        SET scoreboard_revision = contest.scoreboard_revision + 1
        WHERE EXISTS (
            SELECT 1
            FROM contest_teams roster
            WHERE roster.contest_id = contest.id
              AND roster.team_id = NEW.id
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_team_scoreboard_revision
AFTER UPDATE OF name, school, star, deleted_at ON teams
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision_for_team();

CREATE OR REPLACE FUNCTION preserve_or_bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status
       OR OLD.start_at IS DISTINCT FROM NEW.start_at
       OR OLD.freeze_at IS DISTINCT FROM NEW.freeze_at
       OR OLD.end_at IS DISTINCT FROM NEW.end_at
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        NEW.scoreboard_revision := OLD.scoreboard_revision + 1;
    ELSE
        NEW.scoreboard_revision := greatest(NEW.scoreboard_revision, OLD.scoreboard_revision);
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_contest_scoreboard_revision
BEFORE UPDATE ON contests
FOR EACH ROW EXECUTE FUNCTION preserve_or_bump_contest_scoreboard_revision();
