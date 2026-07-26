CREATE TABLE public.auth_sessions (
    token_hash character(64) PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    access_fingerprint character(64) NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    last_seen_at timestamp with time zone NOT NULL DEFAULT now(),
    expires_at timestamp with time zone NOT NULL,
    CONSTRAINT ck_auth_sessions_token_hash
        CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT ck_auth_sessions_access_fingerprint
        CHECK (access_fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT ck_auth_sessions_expiry
        CHECK (expires_at > created_at)
);

CREATE INDEX idx_auth_sessions_user_id
    ON public.auth_sessions(user_id);

CREATE INDEX idx_auth_sessions_expires_at
    ON public.auth_sessions(expires_at);

CREATE INDEX idx_audit_logs_login_rate_limit
    ON public.audit_logs(target_id, request_ip, created_at DESC)
    WHERE action = 'auth.login' AND result = 'failed';
