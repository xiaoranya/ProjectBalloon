CREATE TABLE IF NOT EXISTS public.practice_problem_favorites (
    user_id bigint NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE CASCADE,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, problem_id)
);

CREATE TABLE IF NOT EXISTS public.problem_editorials (
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE CASCADE,
    lang_code character varying(8) NOT NULL,
    title character varying(255) NOT NULL,
    body text NOT NULL,
    unlock_policy character varying(24) NOT NULL DEFAULT 'AFTER_ACCEPTED',
    published boolean NOT NULL DEFAULT false,
    updated_by_user_id bigint REFERENCES public.users(id) ON DELETE SET NULL,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (problem_id, lang_code),
    CONSTRAINT problem_editorial_unlock_known CHECK (unlock_policy IN ('ALWAYS', 'AFTER_ATTEMPT', 'AFTER_ACCEPTED')),
    CONSTRAINT problem_editorial_content_valid CHECK (char_length(btrim(title)) BETWEEN 1 AND 255 AND char_length(body) BETWEEN 1 AND 1048576)
);
