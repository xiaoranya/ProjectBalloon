ALTER TABLE public.practice_virtual_sessions
    ADD COLUMN IF NOT EXISTS archived_at timestamp with time zone;
