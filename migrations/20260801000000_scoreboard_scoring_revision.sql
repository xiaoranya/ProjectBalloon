-- Scoring and feedback policy are part of the scoreboard projection contract.
-- Keep the same revision keying used by roster/problem/status changes so a
-- cached public or administrative board cannot outlive a policy update.
CREATE OR REPLACE FUNCTION preserve_or_bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status
       OR OLD.start_at IS DISTINCT FROM NEW.start_at
       OR OLD.freeze_at IS DISTINCT FROM NEW.freeze_at
       OR OLD.end_at IS DISTINCT FROM NEW.end_at
       OR OLD.scoring_mode IS DISTINCT FROM NEW.scoring_mode
       OR OLD.score_aggregation IS DISTINCT FROM NEW.score_aggregation
       OR OLD.feedback_policy IS DISTINCT FROM NEW.feedback_policy
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        NEW.scoreboard_revision := OLD.scoreboard_revision + 1;
    ELSE
        NEW.scoreboard_revision := greatest(NEW.scoreboard_revision, OLD.scoreboard_revision);
    END IF;
    RETURN NEW;
END;
$$;
