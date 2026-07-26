ALTER TABLE contest_problems
    DROP CONSTRAINT contest_problems_contest_id_display_order_key,
    ADD CONSTRAINT contest_problems_contest_id_display_order_key
        UNIQUE (contest_id, display_order)
        DEFERRABLE INITIALLY IMMEDIATE,
    ADD CONSTRAINT contest_problems_alias_format_check
        CHECK (alias ~ '^[A-Z0-9]{1,8}$'),
    ADD CONSTRAINT contest_problems_display_order_check
        CHECK (display_order BETWEEN 1 AND 1000);
