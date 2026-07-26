UPDATE award_categories SET code = upper(btrim(code)), name = btrim(name);
UPDATE award_categories SET participation_type = upper(participation_type)
WHERE participation_type IS NOT NULL;

ALTER TABLE award_categories
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT award_category_text_bounds CHECK (
        char_length(code) BETWEEN 1 AND 64 AND code ~ '^[A-Z0-9_]+$'
        AND char_length(name) BETWEEN 1 AND 128
    ),
    ADD CONSTRAINT award_category_order_valid CHECK (display_order BETWEEN 1 AND 1000),
    ADD CONSTRAINT award_category_participation_known CHECK (
        participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')
    );

CREATE UNIQUE INDEX uq_award_category_display_order
    ON award_categories (contest_id, display_order);

DELETE FROM award_rules rule
USING award_rules newer
WHERE rule.category_id = newer.category_id AND rule.id > newer.id;

ALTER TABLE award_rules
    ADD CONSTRAINT award_rule_type_known
        CHECK (rule_type IN ('FIXED_COUNT', 'RATIO', 'RANK_RANGE')),
    ADD CONSTRAINT award_rule_shape CHECK (
        (rule_type = 'FIXED_COUNT' AND fixed_count BETWEEN 1 AND 10000
            AND ratio IS NULL AND rank_from IS NULL AND rank_to IS NULL)
        OR (rule_type = 'RATIO' AND ratio > 0 AND ratio <= 1
            AND fixed_count IS NULL AND rank_from IS NULL AND rank_to IS NULL)
        OR (rule_type = 'RANK_RANGE' AND rank_from >= 1 AND rank_to >= rank_from
            AND fixed_count IS NULL AND ratio IS NULL)
    );

CREATE UNIQUE INDEX uq_award_rule_category ON award_rules (category_id);

CREATE TABLE award_sets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    contest_id bigint NOT NULL UNIQUE REFERENCES contests(id),
    resolver_run_id bigint NOT NULL UNIQUE REFERENCES resolver_runs(id),
    final_scoreboard_snapshot_id bigint NOT NULL REFERENCES scoreboard_snapshots(id),
    status varchar(16) NOT NULL DEFAULT 'DRAFT',
    generated_by_user_id bigint NOT NULL REFERENCES users(id),
    frozen_by_user_id bigint REFERENCES users(id),
    generated_at timestamptz NOT NULL DEFAULT now(),
    frozen_at timestamptz,
    version integer NOT NULL DEFAULT 0,
    CONSTRAINT award_set_status_known CHECK (status IN ('DRAFT', 'FROZEN')),
    CONSTRAINT award_set_freeze_shape CHECK (
        (status = 'DRAFT' AND frozen_at IS NULL AND frozen_by_user_id IS NULL)
        OR (status = 'FROZEN' AND frozen_at IS NOT NULL AND frozen_by_user_id IS NOT NULL)
    )
);

ALTER TABLE award_recipients
    ADD COLUMN source_scoreboard_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT award_recipient_participation_known CHECK (
        participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')
    );

CREATE INDEX idx_award_recipients_team_conflicts
    ON award_recipients (contest_id, team_id, category_id);
