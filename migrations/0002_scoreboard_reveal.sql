-- Explicit public scoreboard reveal (the "unfreeze" ceremony).
--
-- Freezing is a one-way milestone driven by `freeze_at`; reaching `end_at` must
-- NOT thaw the public board, otherwise the full final ranking leaks before the
-- official Resolver reveal (see docs/architecture/scoreboard-resolver.md).
-- Revealing is therefore an explicit, audited administrative action, recorded
-- on the contest so every read path agrees on the board's visibility.

ALTER TABLE contests
    ADD COLUMN scoreboard_revealed_at timestamp with time zone,
    ADD COLUMN scoreboard_revealed_by_user_id bigint REFERENCES users(id);

ALTER TABLE contests
    ADD CONSTRAINT ck_contests_scoreboard_reveal_shape CHECK (
        (scoreboard_revealed_at IS NULL AND scoreboard_revealed_by_user_id IS NULL)
        OR (scoreboard_revealed_at IS NOT NULL AND scoreboard_revealed_by_user_id IS NOT NULL)
    );

COMMENT ON COLUMN contests.scoreboard_revealed_at IS
    'When the public scoreboard was explicitly revealed; NULL means the board stays frozen at freeze_at.';

COMMENT ON COLUMN contests.scoreboard_revealed_by_user_id IS
    'Operator who revealed the public scoreboard; set together with scoreboard_revealed_at.';
