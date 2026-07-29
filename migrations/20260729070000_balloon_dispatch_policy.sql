CREATE TABLE IF NOT EXISTS public.balloon_dispatch_policies (
    contest_id bigint PRIMARY KEY REFERENCES public.contests(id) ON DELETE CASCADE,
    strategy character varying(16) NOT NULL DEFAULT 'PRIORITY',
    max_batch integer NOT NULL DEFAULT 10,
    cooldown_seconds integer NOT NULL DEFAULT 0,
    zone_order text NOT NULL DEFAULT '[]',
    updated_by_user_id bigint REFERENCES public.users(id),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT balloon_dispatch_strategy_known CHECK (strategy IN ('FIFO', 'PRIORITY', 'ZONE')),
    CONSTRAINT balloon_dispatch_limits_valid CHECK (max_batch BETWEEN 1 AND 100 AND cooldown_seconds BETWEEN 0 AND 3600),
    CONSTRAINT balloon_dispatch_zone_order_json CHECK (jsonb_typeof(zone_order::jsonb) = 'array')
);

ALTER TABLE public.balloon_tasks
    ADD COLUMN IF NOT EXISTS priority integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS delivery_zone character varying(64) NOT NULL DEFAULT 'DEFAULT',
    ADD COLUMN IF NOT EXISTS dispatch_attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_dispatched_at timestamp with time zone;

ALTER TABLE public.balloon_tasks
    ADD CONSTRAINT balloon_task_dispatch_values_valid CHECK (priority BETWEEN -1000 AND 1000 AND dispatch_attempts >= 0);
CREATE INDEX IF NOT EXISTS idx_balloon_tasks_dispatch_queue
    ON public.balloon_tasks (contest_id, status, priority DESC, created_at, id);
