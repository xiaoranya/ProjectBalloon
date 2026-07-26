ALTER TABLE problem_testdata_versions
    ADD COLUMN case_count integer,
    ADD CONSTRAINT problem_testdata_versions_case_count_positive
        CHECK (case_count IS NULL OR case_count > 0);

COMMENT ON COLUMN problem_testdata_versions.case_count IS
    'Validated number of root-level .in/.out pairs; NULL only for bridged legacy versions.';
