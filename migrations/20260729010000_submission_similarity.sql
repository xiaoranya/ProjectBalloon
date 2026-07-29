ALTER TABLE submissions
    ADD COLUMN source_fingerprint varchar(64),
    ADD COLUMN source_simhash bigint,
    ADD COLUMN source_token_count integer;

ALTER TABLE submissions
    ADD CONSTRAINT submissions_source_fingerprint_format
    CHECK (source_fingerprint IS NULL OR source_fingerprint ~ '^[0-9a-f]{64}$');

ALTER TABLE submissions
    ADD CONSTRAINT submissions_similarity_shape
    CHECK (
        (source_simhash IS NULL AND source_token_count IS NULL)
        OR (source_simhash IS NOT NULL AND source_token_count > 0)
    );

CREATE INDEX idx_submissions_similarity
    ON submissions (contest_id, problem_id, language, source_fingerprint)
    WHERE source_fingerprint IS NOT NULL;

CREATE INDEX idx_submissions_simhash
    ON submissions (contest_id, problem_id, language, source_simhash)
    WHERE source_simhash IS NOT NULL;

COMMENT ON COLUMN submissions.source_fingerprint IS
    'SHA-256 of source with comments and formatting whitespace removed; used for exact normalized duplicate detection';

COMMENT ON COLUMN submissions.source_simhash IS
    '64-bit SimHash over normalized five-token shingles for approximate similarity screening';
