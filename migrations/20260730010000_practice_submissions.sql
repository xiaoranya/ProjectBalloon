ALTER TABLE public.submissions
    ALTER COLUMN contest_id DROP NOT NULL,
    ALTER COLUMN team_id DROP NOT NULL;

ALTER TABLE public.submissions
    ADD COLUMN IF NOT EXISTS submission_scope character varying(16) NOT NULL DEFAULT 'CONTEST',
    ADD COLUMN IF NOT EXISTS participant_user_id bigint REFERENCES public.users(id),
    ADD COLUMN IF NOT EXISTS training_enrollment_id bigint REFERENCES public.training_enrollments(id) ON DELETE SET NULL;

ALTER TABLE public.submissions
    ADD CONSTRAINT submissions_scope_known CHECK (submission_scope IN ('CONTEST', 'PRACTICE'));
ALTER TABLE public.submissions
    ADD CONSTRAINT submissions_scope_shape CHECK (
        (submission_scope = 'CONTEST' AND contest_id IS NOT NULL AND team_id IS NOT NULL)
        OR (submission_scope = 'PRACTICE' AND contest_id IS NULL AND participant_user_id IS NOT NULL)
    );

CREATE INDEX IF NOT EXISTS idx_submissions_practice_user
    ON public.submissions (participant_user_id, submitted_at DESC, id DESC)
    WHERE submission_scope = 'PRACTICE';
CREATE INDEX IF NOT EXISTS idx_submissions_training
    ON public.submissions (training_enrollment_id, problem_id, submitted_at DESC)
    WHERE submission_scope = 'PRACTICE' AND training_enrollment_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS public.practice_problem_progress (
    user_id bigint NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE CASCADE,
    attempts integer NOT NULL DEFAULT 0,
    best_score integer NOT NULL DEFAULT 0,
    solved boolean NOT NULL DEFAULT false,
    last_submission_id bigint REFERENCES public.submissions(id) ON DELETE SET NULL,
    solved_at timestamp with time zone,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, problem_id),
    CONSTRAINT practice_progress_values_valid CHECK (attempts >= 0 AND best_score BETWEEN 0 AND 100)
);

ALTER TABLE public.training_enrollments
    ALTER COLUMN team_id DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS user_id bigint REFERENCES public.users(id) ON DELETE CASCADE;
ALTER TABLE public.training_enrollments
    ADD CONSTRAINT training_enrollment_owner_shape CHECK ((team_id IS NOT NULL) <> (user_id IS NOT NULL));
CREATE UNIQUE INDEX IF NOT EXISTS idx_training_enrollments_user ON public.training_enrollments(set_id,user_id) WHERE user_id IS NOT NULL;
