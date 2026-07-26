ALTER TABLE judgements
    ADD COLUMN result_message_id uuid;

CREATE UNIQUE INDEX uq_judgements_result_message_id
    ON judgements (result_message_id)
    WHERE result_message_id IS NOT NULL;

CREATE UNIQUE INDEX uq_runs_judgement_test_index
    ON runs (judgement_id, test_index);
