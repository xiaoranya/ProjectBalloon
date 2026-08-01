CREATE INDEX IF NOT EXISTS idx_practice_progress_user_updated
    ON public.practice_problem_progress (user_id, updated_at DESC, problem_id);
