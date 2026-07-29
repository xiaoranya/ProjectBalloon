ALTER TABLE problems
    ADD COLUMN judge_mode varchar(20) NOT NULL DEFAULT 'STANDARD',
    ADD COLUMN interactor_object_key varchar(512),
    ADD COLUMN interactor_sha256 varchar(64),
    ADD CONSTRAINT problems_judge_mode_known
        CHECK (judge_mode IN ('STANDARD', 'INTERACTIVE', 'OUTPUT_ONLY')),
    ADD CONSTRAINT problems_interactor_pair
        CHECK ((judge_mode = 'INTERACTIVE' AND interactor_object_key IS NOT NULL
                AND interactor_sha256 ~ '^[0-9a-f]{64}$')
            OR (judge_mode <> 'INTERACTIVE' AND interactor_object_key IS NULL
                AND interactor_sha256 IS NULL)),
    ADD CONSTRAINT problems_mode_language_shape CHECK (
        (judge_mode = 'OUTPUT_ONLY' AND languages::jsonb = '["output"]'::jsonb)
        OR (judge_mode <> 'OUTPUT_ONLY' AND NOT languages::jsonb ? 'output')
    );

ALTER TABLE submissions
    ADD CONSTRAINT submissions_output_language_allowed
        CHECK (language <> 'output' OR source_object_key ~ '[.]zip$');
