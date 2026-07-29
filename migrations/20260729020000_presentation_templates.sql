ALTER TABLE presentation_configs
    ADD COLUMN template varchar(32) NOT NULL DEFAULT 'DEFAULT';

ALTER TABLE presentation_configs
    ADD CONSTRAINT presentation_configs_template_known
    CHECK (template IN ('DEFAULT', 'CINEMATIC', 'MINIMAL', 'SPLIT'));
