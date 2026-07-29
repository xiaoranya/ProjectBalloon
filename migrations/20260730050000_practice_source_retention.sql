ALTER TABLE public.submissions
    ADD COLUMN IF NOT EXISTS source_deleted_at timestamp with time zone;

CREATE INDEX IF NOT EXISTS submissions_practice_retention_idx
    ON public.submissions (submitted_at)
    WHERE submission_scope = 'PRACTICE' AND source_deleted_at IS NULL;
