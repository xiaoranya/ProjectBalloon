CREATE TABLE IF NOT EXISTS public.practice_platform_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    daily_submission_limit integer NOT NULL DEFAULT 200,
    concurrent_judging_limit integer NOT NULL DEFAULT 3,
    source_retention_days integer NOT NULL DEFAULT 365,
    updated_by_user_id bigint REFERENCES public.users(id) ON DELETE SET NULL,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT practice_limits_valid CHECK (daily_submission_limit BETWEEN 1 AND 10000 AND concurrent_judging_limit BETWEEN 1 AND 20 AND source_retention_days BETWEEN 1 AND 3650)
);
INSERT INTO public.practice_platform_settings(singleton) VALUES(true) ON CONFLICT DO NOTHING;
