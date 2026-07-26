-- ProjectBalloon fresh-install baseline.
--
-- Generated from the effective schema after applying the previous V001-V031
-- migrations to PostgreSQL, then reviewed for SQLx use. This migration is for
-- a new Rust installation. Existing databases require the migration-ledger
-- bridge described in docs/architecture/ADR-002-rust-backend-reset.md.

--
-- Name: announcements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.announcements (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    title character varying(255) NOT NULL,
    body text NOT NULL,
    pinned boolean DEFAULT false NOT NULL,
    status character varying(16) DEFAULT 'published'::character varying NOT NULL,
    created_by bigint NOT NULL,
    source_clarification_id bigint,
    published_at timestamp with time zone DEFAULT now(),
    withdrawn_at timestamp with time zone,
    withdrawn_by bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    scheduled_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    cancelled_by bigint
);


--
-- Name: announcements_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.announcements_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: announcements_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.announcements_id_seq OWNED BY public.announcements.id;


--
-- Name: audit_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.audit_logs (
    id bigint NOT NULL,
    actor_user_id bigint,
    action character varying(128) NOT NULL,
    target_type character varying(128),
    target_id character varying(128),
    request_ip character varying(64),
    result character varying(32) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: audit_logs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.audit_logs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: audit_logs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.audit_logs_id_seq OWNED BY public.audit_logs.id;


--
-- Name: award_categories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_categories (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    code character varying(64) NOT NULL,
    name character varying(128) NOT NULL,
    display_order integer NOT NULL,
    include_star boolean DEFAULT true NOT NULL,
    group_name character varying(128),
    first_blood boolean DEFAULT false NOT NULL,
    frozen boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    participation_type character varying(32)
);


--
-- Name: award_categories_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_categories_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_categories_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_categories_id_seq OWNED BY public.award_categories.id;


--
-- Name: award_certificate_rows; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_certificate_rows (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    award_recipient_id bigint NOT NULL,
    source_member_id bigint,
    certificate_no character varying(64) NOT NULL,
    export_order integer NOT NULL,
    contest_name character varying(255) NOT NULL,
    award_code character varying(64) NOT NULL,
    award_name character varying(128) NOT NULL,
    problem_alias character varying(8),
    team_id bigint NOT NULL,
    team_name character varying(255) NOT NULL,
    school character varying(255),
    recipient_name character varying(128) NOT NULL,
    recipient_role character varying(64),
    seat_no character varying(64),
    group_name character varying(128),
    participation_type character varying(32),
    rank integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: award_certificate_rows_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_certificate_rows_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_certificate_rows_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_certificate_rows_id_seq OWNED BY public.award_certificate_rows.id;


--
-- Name: award_host_script_sections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_host_script_sections (
    id bigint NOT NULL,
    host_script_id bigint NOT NULL,
    category_id bigint NOT NULL,
    cue_text character varying(2000) NOT NULL,
    display_order integer NOT NULL
);


--
-- Name: award_host_script_sections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_host_script_sections_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_host_script_sections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_host_script_sections_id_seq OWNED BY public.award_host_script_sections.id;


--
-- Name: award_host_scripts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_host_scripts (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    opening_text character varying(4000) NOT NULL,
    closing_text character varying(4000) NOT NULL,
    updated_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL
);


--
-- Name: award_host_scripts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_host_scripts_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_host_scripts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_host_scripts_id_seq OWNED BY public.award_host_scripts.id;


--
-- Name: award_presentation_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_presentation_states (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    current_category_id bigint,
    status character varying(24) DEFAULT 'WAITING'::character varying NOT NULL,
    auto_rotate boolean DEFAULT false NOT NULL,
    interval_seconds integer DEFAULT 15 NOT NULL,
    updated_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT ck_award_presentation_interval CHECK (((interval_seconds >= 5) AND (interval_seconds <= 120)))
);


--
-- Name: award_presentation_states_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_presentation_states_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_presentation_states_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_presentation_states_id_seq OWNED BY public.award_presentation_states.id;


--
-- Name: award_recipients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_recipients (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    category_id bigint NOT NULL,
    team_id bigint NOT NULL,
    rank integer,
    solved integer,
    penalty_minutes bigint,
    team_name character varying(255),
    school character varying(255),
    group_name character varying(128),
    is_star boolean DEFAULT false NOT NULL,
    is_manual boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    seat_no character varying(64),
    participation_type character varying(32),
    award_key character varying(64) DEFAULT 'TEAM'::character varying NOT NULL,
    problem_id bigint,
    problem_alias character varying(8)
);


--
-- Name: award_recipients_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_recipients_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_recipients_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_recipients_id_seq OWNED BY public.award_recipients.id;


--
-- Name: award_rules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.award_rules (
    id bigint NOT NULL,
    category_id bigint NOT NULL,
    rule_type character varying(32) NOT NULL,
    ratio numeric(5,4),
    fixed_count integer,
    rank_from integer,
    rank_to integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: award_rules_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.award_rules_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: award_rules_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.award_rules_id_seq OWNED BY public.award_rules.id;


--
-- Name: balloon_tasks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.balloon_tasks (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    team_id bigint NOT NULL,
    problem_id bigint NOT NULL,
    submission_id bigint NOT NULL,
    color character varying(16),
    is_first_blood boolean DEFAULT false NOT NULL,
    status character varying(32) DEFAULT 'pending'::character varying NOT NULL,
    seat_no character varying(64),
    team_name character varying(255),
    problem_alias character varying(8),
    note text,
    claimed_by bigint,
    claimed_at timestamp with time zone,
    delivered_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    cancelled_reason character varying(255),
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: balloon_tasks_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.balloon_tasks_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: balloon_tasks_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.balloon_tasks_id_seq OWNED BY public.balloon_tasks.id;


--
-- Name: batch_rejudge_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.batch_rejudge_items (
    id bigint NOT NULL,
    task_id bigint NOT NULL,
    submission_id bigint NOT NULL,
    status character varying(16) NOT NULL,
    old_judgement_id uuid,
    new_judgement_id uuid,
    error_message text,
    processed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: batch_rejudge_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.batch_rejudge_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: batch_rejudge_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.batch_rejudge_items_id_seq OWNED BY public.batch_rejudge_items.id;


--
-- Name: batch_rejudge_tasks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.batch_rejudge_tasks (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    status character varying(32) NOT NULL,
    idempotency_key character varying(128) NOT NULL,
    filter_data text NOT NULL,
    total_items integer DEFAULT 0 NOT NULL,
    processed_items integer DEFAULT 0 NOT NULL,
    succeeded_items integer DEFAULT 0 NOT NULL,
    failed_items integer DEFAULT 0 NOT NULL,
    cancel_requested boolean DEFAULT false NOT NULL,
    created_by_user_id bigint NOT NULL,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: batch_rejudge_tasks_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.batch_rejudge_tasks_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: batch_rejudge_tasks_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.batch_rejudge_tasks_id_seq OWNED BY public.batch_rejudge_tasks.id;


--
-- Name: broadcast_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.broadcast_tokens (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    label character varying(120) NOT NULL,
    token_hash character varying(64) NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    last_used_at timestamp with time zone,
    created_by_user_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: broadcast_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.broadcast_tokens_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: broadcast_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.broadcast_tokens_id_seq OWNED BY public.broadcast_tokens.id;


--
-- Name: clarifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.clarifications (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    team_id bigint NOT NULL,
    team_name character varying(255),
    scope character varying(16) NOT NULL,
    problem_id bigint,
    problem_alias character varying(8),
    question text NOT NULL,
    status character varying(16) DEFAULT 'pending'::character varying NOT NULL,
    reply text,
    reply_visibility character varying(16),
    asked_by bigint NOT NULL,
    replied_by bigint,
    replied_at timestamp with time zone,
    converted_announcement_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: clarifications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.clarifications_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: clarifications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.clarifications_id_seq OWNED BY public.clarifications.id;


--
-- Name: contest_admin_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contest_admin_assignments (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    contest_id bigint NOT NULL,
    assigned_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contest_admin_assignments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.contest_admin_assignments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: contest_admin_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.contest_admin_assignments_id_seq OWNED BY public.contest_admin_assignments.id;


--
-- Name: contest_problems; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contest_problems (
    contest_id bigint NOT NULL,
    problem_id bigint NOT NULL,
    alias character varying(8) NOT NULL,
    display_order integer NOT NULL,
    color character varying(16),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contest_teams; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contest_teams (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    team_id bigint NOT NULL,
    participation_type character varying(32) NOT NULL,
    group_name character varying(128),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contest_teams_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.contest_teams_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: contest_teams_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.contest_teams_id_seq OWNED BY public.contest_teams.id;


--
-- Name: contests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contests (
    id bigint NOT NULL,
    name character varying(255) NOT NULL,
    status character varying(32) NOT NULL,
    visibility character varying(32) NOT NULL,
    start_at timestamp with time zone,
    end_at timestamp with time zone,
    freeze_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    version bigint DEFAULT 0 NOT NULL
);


--
-- Name: contests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.contests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: contests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.contests_id_seq OWNED BY public.contests.id;


--
-- Name: judgements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.judgements (
    id uuid NOT NULL,
    submission_id bigint NOT NULL,
    verdict character varying(32),
    total_time_ms integer,
    peak_memory_kb integer,
    compile_log text,
    worker_id character varying(64),
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    version integer DEFAULT 0 NOT NULL,
    superseded boolean DEFAULT false NOT NULL,
    active_marker boolean DEFAULT true,
    CONSTRAINT ck_judgements_active_marker CHECK ((((superseded = false) AND (active_marker = true)) OR ((superseded = true) AND (active_marker IS NULL))))
);


--
-- Name: permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.permissions (
    id bigint NOT NULL,
    code character varying(128) NOT NULL,
    name character varying(128) NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: permissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.permissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: permissions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.permissions_id_seq OWNED BY public.permissions.id;


--
-- Name: presentation_configs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.presentation_configs (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    mode character varying(16) NOT NULL,
    enabled boolean DEFAULT false NOT NULL,
    title character varying(160),
    subtitle character varying(240),
    accent_color character varying(16) DEFAULT '#22c55e'::character varying NOT NULL,
    row_limit integer DEFAULT 12 NOT NULL,
    show_announcements boolean DEFAULT true NOT NULL,
    announcement_interval_seconds integer DEFAULT 10 NOT NULL,
    updated_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: presentation_configs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.presentation_configs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: presentation_configs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.presentation_configs_id_seq OWNED BY public.presentation_configs.id;


--
-- Name: print_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.print_requests (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    team_id bigint NOT NULL,
    team_name character varying(255),
    seat_no character varying(64),
    content text NOT NULL,
    content_hash character varying(64) NOT NULL,
    page_count integer NOT NULL,
    status character varying(32) DEFAULT 'REQUESTED'::character varying NOT NULL,
    printer_id character varying(128),
    cups_job_id character varying(128),
    pdf_object_key character varying(255),
    pdf_bucket character varying(128),
    requested_by bigint NOT NULL,
    operator_user_id bigint,
    request_ip character varying(64),
    completed_at timestamp with time zone,
    failed_reason character varying(255),
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: print_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.print_requests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: print_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.print_requests_id_seq OWNED BY public.print_requests.id;


--
-- Name: problem_attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.problem_attachments (
    id bigint NOT NULL,
    problem_id bigint NOT NULL,
    kind character varying(32) NOT NULL,
    object_key character varying(512) NOT NULL,
    original_filename character varying(255) NOT NULL,
    content_type character varying(127),
    bytes bigint NOT NULL,
    sha256 character varying(64) NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT problem_attachments_kind_check CHECK (((kind)::text = ANY ((ARRAY['SAMPLE'::character varying, 'SUPPLEMENT'::character varying])::text[])))
);


--
-- Name: problem_attachments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.problem_attachments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: problem_attachments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.problem_attachments_id_seq OWNED BY public.problem_attachments.id;


--
-- Name: problem_statements; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.problem_statements (
    problem_id bigint NOT NULL,
    lang_code character varying(8) NOT NULL,
    body text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: problems; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.problems (
    id bigint NOT NULL,
    slug character varying(64) NOT NULL,
    title character varying(255) NOT NULL,
    time_limit_ms integer DEFAULT 1000 NOT NULL,
    memory_limit_mb integer DEFAULT 256 NOT NULL,
    output_limit_kb integer DEFAULT 65536 NOT NULL,
    languages text DEFAULT '["c","cpp","java","python","pypy3"]'::text NOT NULL,
    testdata_version integer DEFAULT 0 NOT NULL,
    testdata_object_key character varying(512),
    testdata_sha256 character varying(64),
    default_lang_code character varying(8) DEFAULT 'en'::character varying NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    created_by bigint,
    deleted_at timestamp with time zone
);


--
-- Name: problems_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.problems_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: problems_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.problems_id_seq OWNED BY public.problems.id;


--
-- Name: resolver_current_state; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resolver_current_state (
    id bigint NOT NULL,
    run_id bigint NOT NULL,
    step_index integer NOT NULL,
    state_data text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: resolver_current_state_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resolver_current_state_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resolver_current_state_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resolver_current_state_id_seq OWNED BY public.resolver_current_state.id;


--
-- Name: resolver_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resolver_events (
    id bigint NOT NULL,
    run_id bigint NOT NULL,
    event_type character varying(64) NOT NULL,
    payload text NOT NULL,
    sequence integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: resolver_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resolver_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resolver_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resolver_events_id_seq OWNED BY public.resolver_events.id;


--
-- Name: resolver_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resolver_runs (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    official boolean DEFAULT false NOT NULL,
    status character varying(32) DEFAULT 'READY'::character varying NOT NULL,
    current_step integer DEFAULT 0 NOT NULL,
    total_steps integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: resolver_runs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resolver_runs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resolver_runs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resolver_runs_id_seq OWNED BY public.resolver_runs.id;


--
-- Name: resolver_snapshots; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resolver_snapshots (
    id bigint NOT NULL,
    run_id bigint NOT NULL,
    step_index integer NOT NULL,
    state_data text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: resolver_snapshots_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resolver_snapshots_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resolver_snapshots_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resolver_snapshots_id_seq OWNED BY public.resolver_snapshots.id;


--
-- Name: resolver_team_states; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.resolver_team_states (
    id bigint NOT NULL,
    run_id bigint NOT NULL,
    team_id bigint NOT NULL,
    step_index integer NOT NULL,
    state_data text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: resolver_team_states_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.resolver_team_states_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: resolver_team_states_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.resolver_team_states_id_seq OWNED BY public.resolver_team_states.id;


--
-- Name: role_permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.role_permissions (
    role_id bigint NOT NULL,
    permission_id bigint NOT NULL
);


--
-- Name: roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.roles (
    id bigint NOT NULL,
    code character varying(64) NOT NULL,
    name character varying(128) NOT NULL,
    description text,
    builtin boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: roles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.roles_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: roles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.roles_id_seq OWNED BY public.roles.id;


--
-- Name: runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.runs (
    id bigint NOT NULL,
    judgement_id uuid NOT NULL,
    test_index integer NOT NULL,
    verdict character varying(32),
    time_ms integer,
    memory_kb integer,
    exit_code integer,
    stderr_tail text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: runs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.runs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: runs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.runs_id_seq OWNED BY public.runs.id;


--
-- Name: scoreboard_snapshots; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.scoreboard_snapshots (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    variant character varying(32) NOT NULL,
    group_name character varying(128),
    version bigint NOT NULL,
    frozen boolean NOT NULL,
    generated_at timestamp with time zone NOT NULL,
    payload_json text NOT NULL,
    created_by character varying(128),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: scoreboard_snapshots_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.scoreboard_snapshots_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: scoreboard_snapshots_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.scoreboard_snapshots_id_seq OWNED BY public.scoreboard_snapshots.id;


--
-- Name: screen_commands; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_commands (
    id bigint NOT NULL,
    screen_instance_id bigint NOT NULL,
    target_view character varying(32) NOT NULL,
    created_by_user_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    acknowledged_at timestamp with time zone
);


--
-- Name: screen_commands_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_commands_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_commands_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_commands_id_seq OWNED BY public.screen_commands.id;


--
-- Name: screen_group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_group_members (
    id bigint NOT NULL,
    group_id bigint NOT NULL,
    screen_instance_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: screen_group_members_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_group_members_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_group_members_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_group_members_id_seq OWNED BY public.screen_group_members.id;


--
-- Name: screen_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_groups (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    name character varying(120) NOT NULL,
    playlist_id bigint,
    playback_status character varying(16) DEFAULT 'STOPPED'::character varying NOT NULL,
    playback_started_at timestamp with time zone,
    paused_elapsed_seconds bigint DEFAULT 0 NOT NULL,
    locked_view character varying(32),
    created_by_user_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL
);


--
-- Name: screen_groups_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_groups_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_groups_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_groups_id_seq OWNED BY public.screen_groups.id;


--
-- Name: screen_instances; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_instances (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    name character varying(120) NOT NULL,
    client_token_hash character varying(64) NOT NULL,
    current_view character varying(32) DEFAULT 'SCOREBOARD'::character varying NOT NULL,
    last_seen_at timestamp with time zone,
    last_ip character varying(64),
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: screen_instances_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_instances_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_instances_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_instances_id_seq OWNED BY public.screen_instances.id;


--
-- Name: screen_playlist_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_playlist_items (
    id bigint NOT NULL,
    playlist_id bigint NOT NULL,
    target_view character varying(32) NOT NULL,
    duration_seconds integer NOT NULL,
    display_order integer NOT NULL,
    CONSTRAINT screen_playlist_items_duration_seconds_check CHECK (((duration_seconds >= 5) AND (duration_seconds <= 3600)))
);


--
-- Name: screen_playlist_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_playlist_items_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_playlist_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_playlist_items_id_seq OWNED BY public.screen_playlist_items.id;


--
-- Name: screen_playlists; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.screen_playlists (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    name character varying(120) NOT NULL,
    loop_enabled boolean DEFAULT true NOT NULL,
    created_by_user_id bigint NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL
);


--
-- Name: screen_playlists_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.screen_playlists_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: screen_playlists_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.screen_playlists_id_seq OWNED BY public.screen_playlists.id;


--
-- Name: submission_outbox; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.submission_outbox (
    id bigint NOT NULL,
    judgement_id uuid NOT NULL,
    submission_id bigint NOT NULL,
    payload text NOT NULL,
    status character varying(16) DEFAULT 'PENDING'::character varying NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    sent_at timestamp with time zone,
    version integer DEFAULT 0 NOT NULL
);


--
-- Name: submission_outbox_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.submission_outbox_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: submission_outbox_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.submission_outbox_id_seq OWNED BY public.submission_outbox.id;


--
-- Name: submissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.submissions (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    problem_id bigint NOT NULL,
    team_id bigint NOT NULL,
    language character varying(16) NOT NULL,
    source_object_key character varying(512) NOT NULL,
    source_size_bytes integer NOT NULL,
    status character varying(32) DEFAULT 'PENDING'::character varying NOT NULL,
    submitted_at timestamp with time zone DEFAULT now() NOT NULL,
    judged_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: submissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.submissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: submissions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.submissions_id_seq OWNED BY public.submissions.id;


--
-- Name: team_import_batches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.team_import_batches (
    id bigint NOT NULL,
    batch_id character varying(36) NOT NULL,
    idempotency_key character varying(128) NOT NULL,
    request_data text NOT NULL,
    response_data text,
    created_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone
);


--
-- Name: team_import_batches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.team_import_batches_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: team_import_batches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.team_import_batches_id_seq OWNED BY public.team_import_batches.id;


--
-- Name: team_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.team_members (
    id bigint NOT NULL,
    team_id bigint NOT NULL,
    name character varying(128) NOT NULL,
    email character varying(255),
    phone character varying(64),
    role_name character varying(64),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: team_members_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.team_members_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: team_members_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.team_members_id_seq OWNED BY public.team_members.id;


--
-- Name: teams; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.teams (
    id bigint NOT NULL,
    name character varying(255) NOT NULL,
    school character varying(255),
    seat_no character varying(64),
    group_name character varying(128),
    star boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone
);


--
-- Name: teams_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.teams_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: teams_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.teams_id_seq OWNED BY public.teams.id;


--
-- Name: user_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_roles (
    user_id bigint NOT NULL,
    role_id bigint NOT NULL
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id bigint NOT NULL,
    username character varying(64) NOT NULL,
    password_hash character varying(255) NOT NULL,
    display_name character varying(128) NOT NULL,
    user_type character varying(32) NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    password_reset_required boolean DEFAULT false NOT NULL,
    last_login_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.users_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: announcements id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.announcements ALTER COLUMN id SET DEFAULT nextval('public.announcements_id_seq'::regclass);


--
-- Name: audit_logs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.audit_logs ALTER COLUMN id SET DEFAULT nextval('public.audit_logs_id_seq'::regclass);


--
-- Name: award_categories id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_categories ALTER COLUMN id SET DEFAULT nextval('public.award_categories_id_seq'::regclass);


--
-- Name: award_certificate_rows id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows ALTER COLUMN id SET DEFAULT nextval('public.award_certificate_rows_id_seq'::regclass);


--
-- Name: award_host_script_sections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections ALTER COLUMN id SET DEFAULT nextval('public.award_host_script_sections_id_seq'::regclass);


--
-- Name: award_host_scripts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_scripts ALTER COLUMN id SET DEFAULT nextval('public.award_host_scripts_id_seq'::regclass);


--
-- Name: award_presentation_states id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states ALTER COLUMN id SET DEFAULT nextval('public.award_presentation_states_id_seq'::regclass);


--
-- Name: award_recipients id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients ALTER COLUMN id SET DEFAULT nextval('public.award_recipients_id_seq'::regclass);


--
-- Name: award_rules id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_rules ALTER COLUMN id SET DEFAULT nextval('public.award_rules_id_seq'::regclass);


--
-- Name: balloon_tasks id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks ALTER COLUMN id SET DEFAULT nextval('public.balloon_tasks_id_seq'::regclass);


--
-- Name: batch_rejudge_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_items ALTER COLUMN id SET DEFAULT nextval('public.batch_rejudge_items_id_seq'::regclass);


--
-- Name: batch_rejudge_tasks id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_tasks ALTER COLUMN id SET DEFAULT nextval('public.batch_rejudge_tasks_id_seq'::regclass);


--
-- Name: broadcast_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.broadcast_tokens ALTER COLUMN id SET DEFAULT nextval('public.broadcast_tokens_id_seq'::regclass);


--
-- Name: clarifications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.clarifications ALTER COLUMN id SET DEFAULT nextval('public.clarifications_id_seq'::regclass);


--
-- Name: contest_admin_assignments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments ALTER COLUMN id SET DEFAULT nextval('public.contest_admin_assignments_id_seq'::regclass);


--
-- Name: contest_teams id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams ALTER COLUMN id SET DEFAULT nextval('public.contest_teams_id_seq'::regclass);


--
-- Name: contests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contests ALTER COLUMN id SET DEFAULT nextval('public.contests_id_seq'::regclass);


--
-- Name: permissions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permissions ALTER COLUMN id SET DEFAULT nextval('public.permissions_id_seq'::regclass);


--
-- Name: presentation_configs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presentation_configs ALTER COLUMN id SET DEFAULT nextval('public.presentation_configs_id_seq'::regclass);


--
-- Name: print_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.print_requests ALTER COLUMN id SET DEFAULT nextval('public.print_requests_id_seq'::regclass);


--
-- Name: problem_attachments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problem_attachments ALTER COLUMN id SET DEFAULT nextval('public.problem_attachments_id_seq'::regclass);


--
-- Name: problems id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problems ALTER COLUMN id SET DEFAULT nextval('public.problems_id_seq'::regclass);


--
-- Name: resolver_current_state id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_current_state ALTER COLUMN id SET DEFAULT nextval('public.resolver_current_state_id_seq'::regclass);


--
-- Name: resolver_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_events ALTER COLUMN id SET DEFAULT nextval('public.resolver_events_id_seq'::regclass);


--
-- Name: resolver_runs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_runs ALTER COLUMN id SET DEFAULT nextval('public.resolver_runs_id_seq'::regclass);


--
-- Name: resolver_snapshots id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_snapshots ALTER COLUMN id SET DEFAULT nextval('public.resolver_snapshots_id_seq'::regclass);


--
-- Name: resolver_team_states id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_team_states ALTER COLUMN id SET DEFAULT nextval('public.resolver_team_states_id_seq'::regclass);


--
-- Name: roles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles ALTER COLUMN id SET DEFAULT nextval('public.roles_id_seq'::regclass);


--
-- Name: runs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runs ALTER COLUMN id SET DEFAULT nextval('public.runs_id_seq'::regclass);


--
-- Name: scoreboard_snapshots id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scoreboard_snapshots ALTER COLUMN id SET DEFAULT nextval('public.scoreboard_snapshots_id_seq'::regclass);


--
-- Name: screen_commands id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_commands ALTER COLUMN id SET DEFAULT nextval('public.screen_commands_id_seq'::regclass);


--
-- Name: screen_group_members id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_group_members ALTER COLUMN id SET DEFAULT nextval('public.screen_group_members_id_seq'::regclass);


--
-- Name: screen_groups id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups ALTER COLUMN id SET DEFAULT nextval('public.screen_groups_id_seq'::regclass);


--
-- Name: screen_instances id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_instances ALTER COLUMN id SET DEFAULT nextval('public.screen_instances_id_seq'::regclass);


--
-- Name: screen_playlist_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlist_items ALTER COLUMN id SET DEFAULT nextval('public.screen_playlist_items_id_seq'::regclass);


--
-- Name: screen_playlists id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlists ALTER COLUMN id SET DEFAULT nextval('public.screen_playlists_id_seq'::regclass);


--
-- Name: submission_outbox id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submission_outbox ALTER COLUMN id SET DEFAULT nextval('public.submission_outbox_id_seq'::regclass);


--
-- Name: submissions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submissions ALTER COLUMN id SET DEFAULT nextval('public.submissions_id_seq'::regclass);


--
-- Name: team_import_batches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_import_batches ALTER COLUMN id SET DEFAULT nextval('public.team_import_batches_id_seq'::regclass);


--
-- Name: team_members id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_members ALTER COLUMN id SET DEFAULT nextval('public.team_members_id_seq'::regclass);


--
-- Name: teams id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.teams ALTER COLUMN id SET DEFAULT nextval('public.teams_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Name: announcements announcements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.announcements
    ADD CONSTRAINT announcements_pkey PRIMARY KEY (id);


--
-- Name: audit_logs audit_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.audit_logs
    ADD CONSTRAINT audit_logs_pkey PRIMARY KEY (id);


--
-- Name: award_categories award_categories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_categories
    ADD CONSTRAINT award_categories_pkey PRIMARY KEY (id);


--
-- Name: award_certificate_rows award_certificate_rows_certificate_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows
    ADD CONSTRAINT award_certificate_rows_certificate_no_key UNIQUE (certificate_no);


--
-- Name: award_certificate_rows award_certificate_rows_contest_id_export_order_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows
    ADD CONSTRAINT award_certificate_rows_contest_id_export_order_key UNIQUE (contest_id, export_order);


--
-- Name: award_certificate_rows award_certificate_rows_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows
    ADD CONSTRAINT award_certificate_rows_pkey PRIMARY KEY (id);


--
-- Name: award_host_script_sections award_host_script_sections_host_script_id_category_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections
    ADD CONSTRAINT award_host_script_sections_host_script_id_category_id_key UNIQUE (host_script_id, category_id);


--
-- Name: award_host_script_sections award_host_script_sections_host_script_id_display_order_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections
    ADD CONSTRAINT award_host_script_sections_host_script_id_display_order_key UNIQUE (host_script_id, display_order);


--
-- Name: award_host_script_sections award_host_script_sections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections
    ADD CONSTRAINT award_host_script_sections_pkey PRIMARY KEY (id);


--
-- Name: award_host_scripts award_host_scripts_contest_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_scripts
    ADD CONSTRAINT award_host_scripts_contest_id_key UNIQUE (contest_id);


--
-- Name: award_host_scripts award_host_scripts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_scripts
    ADD CONSTRAINT award_host_scripts_pkey PRIMARY KEY (id);


--
-- Name: award_presentation_states award_presentation_states_contest_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states
    ADD CONSTRAINT award_presentation_states_contest_id_key UNIQUE (contest_id);


--
-- Name: award_presentation_states award_presentation_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states
    ADD CONSTRAINT award_presentation_states_pkey PRIMARY KEY (id);


--
-- Name: award_recipients award_recipients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients
    ADD CONSTRAINT award_recipients_pkey PRIMARY KEY (id);


--
-- Name: award_rules award_rules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_rules
    ADD CONSTRAINT award_rules_pkey PRIMARY KEY (id);


--
-- Name: balloon_tasks balloon_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks
    ADD CONSTRAINT balloon_tasks_pkey PRIMARY KEY (id);


--
-- Name: batch_rejudge_items batch_rejudge_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_items
    ADD CONSTRAINT batch_rejudge_items_pkey PRIMARY KEY (id);


--
-- Name: batch_rejudge_tasks batch_rejudge_tasks_idempotency_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_tasks
    ADD CONSTRAINT batch_rejudge_tasks_idempotency_key_key UNIQUE (idempotency_key);


--
-- Name: batch_rejudge_tasks batch_rejudge_tasks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_tasks
    ADD CONSTRAINT batch_rejudge_tasks_pkey PRIMARY KEY (id);


--
-- Name: broadcast_tokens broadcast_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.broadcast_tokens
    ADD CONSTRAINT broadcast_tokens_pkey PRIMARY KEY (id);


--
-- Name: broadcast_tokens broadcast_tokens_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.broadcast_tokens
    ADD CONSTRAINT broadcast_tokens_token_hash_key UNIQUE (token_hash);


--
-- Name: clarifications clarifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.clarifications
    ADD CONSTRAINT clarifications_pkey PRIMARY KEY (id);


--
-- Name: contest_admin_assignments contest_admin_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments
    ADD CONSTRAINT contest_admin_assignments_pkey PRIMARY KEY (id);


--
-- Name: contest_admin_assignments contest_admin_assignments_user_id_contest_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments
    ADD CONSTRAINT contest_admin_assignments_user_id_contest_id_key UNIQUE (user_id, contest_id);


--
-- Name: contest_problems contest_problems_contest_id_alias_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_problems
    ADD CONSTRAINT contest_problems_contest_id_alias_key UNIQUE (contest_id, alias);


--
-- Name: contest_problems contest_problems_contest_id_display_order_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_problems
    ADD CONSTRAINT contest_problems_contest_id_display_order_key UNIQUE (contest_id, display_order);


--
-- Name: contest_problems contest_problems_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_problems
    ADD CONSTRAINT contest_problems_pkey PRIMARY KEY (contest_id, problem_id);


--
-- Name: contest_teams contest_teams_contest_id_team_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams
    ADD CONSTRAINT contest_teams_contest_id_team_id_key UNIQUE (contest_id, team_id);


--
-- Name: contest_teams contest_teams_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams
    ADD CONSTRAINT contest_teams_pkey PRIMARY KEY (id);


--
-- Name: contests contests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contests
    ADD CONSTRAINT contests_pkey PRIMARY KEY (id);


--
-- Name: judgements judgements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.judgements
    ADD CONSTRAINT judgements_pkey PRIMARY KEY (id);


--
-- Name: permissions permissions_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permissions
    ADD CONSTRAINT permissions_code_key UNIQUE (code);


--
-- Name: permissions permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permissions
    ADD CONSTRAINT permissions_pkey PRIMARY KEY (id);


--
-- Name: presentation_configs presentation_configs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presentation_configs
    ADD CONSTRAINT presentation_configs_pkey PRIMARY KEY (id);


--
-- Name: print_requests print_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.print_requests
    ADD CONSTRAINT print_requests_pkey PRIMARY KEY (id);


--
-- Name: problem_attachments problem_attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problem_attachments
    ADD CONSTRAINT problem_attachments_pkey PRIMARY KEY (id);


--
-- Name: problem_statements problem_statements_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problem_statements
    ADD CONSTRAINT problem_statements_pkey PRIMARY KEY (problem_id, lang_code);


--
-- Name: problems problems_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problems
    ADD CONSTRAINT problems_pkey PRIMARY KEY (id);


--
-- Name: problems problems_slug_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problems
    ADD CONSTRAINT problems_slug_key UNIQUE (slug);


--
-- Name: resolver_current_state resolver_current_state_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_current_state
    ADD CONSTRAINT resolver_current_state_pkey PRIMARY KEY (id);


--
-- Name: resolver_current_state resolver_current_state_run_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_current_state
    ADD CONSTRAINT resolver_current_state_run_id_key UNIQUE (run_id);


--
-- Name: resolver_events resolver_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_events
    ADD CONSTRAINT resolver_events_pkey PRIMARY KEY (id);


--
-- Name: resolver_runs resolver_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_runs
    ADD CONSTRAINT resolver_runs_pkey PRIMARY KEY (id);


--
-- Name: resolver_snapshots resolver_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_snapshots
    ADD CONSTRAINT resolver_snapshots_pkey PRIMARY KEY (id);


--
-- Name: resolver_team_states resolver_team_states_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_team_states
    ADD CONSTRAINT resolver_team_states_pkey PRIMARY KEY (id);


--
-- Name: role_permissions role_permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions
    ADD CONSTRAINT role_permissions_pkey PRIMARY KEY (role_id, permission_id);


--
-- Name: roles roles_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_code_key UNIQUE (code);


--
-- Name: roles roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (id);


--
-- Name: runs runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runs
    ADD CONSTRAINT runs_pkey PRIMARY KEY (id);


--
-- Name: scoreboard_snapshots scoreboard_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scoreboard_snapshots
    ADD CONSTRAINT scoreboard_snapshots_pkey PRIMARY KEY (id);


--
-- Name: screen_commands screen_commands_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_commands
    ADD CONSTRAINT screen_commands_pkey PRIMARY KEY (id);


--
-- Name: screen_group_members screen_group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_group_members
    ADD CONSTRAINT screen_group_members_pkey PRIMARY KEY (id);


--
-- Name: screen_group_members screen_group_members_screen_instance_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_group_members
    ADD CONSTRAINT screen_group_members_screen_instance_id_key UNIQUE (screen_instance_id);


--
-- Name: screen_groups screen_groups_contest_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups
    ADD CONSTRAINT screen_groups_contest_id_name_key UNIQUE (contest_id, name);


--
-- Name: screen_groups screen_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups
    ADD CONSTRAINT screen_groups_pkey PRIMARY KEY (id);


--
-- Name: screen_instances screen_instances_client_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_instances
    ADD CONSTRAINT screen_instances_client_token_hash_key UNIQUE (client_token_hash);


--
-- Name: screen_instances screen_instances_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_instances
    ADD CONSTRAINT screen_instances_pkey PRIMARY KEY (id);


--
-- Name: screen_playlist_items screen_playlist_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlist_items
    ADD CONSTRAINT screen_playlist_items_pkey PRIMARY KEY (id);


--
-- Name: screen_playlist_items screen_playlist_items_playlist_id_display_order_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlist_items
    ADD CONSTRAINT screen_playlist_items_playlist_id_display_order_key UNIQUE (playlist_id, display_order);


--
-- Name: screen_playlists screen_playlists_contest_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlists
    ADD CONSTRAINT screen_playlists_contest_id_name_key UNIQUE (contest_id, name);


--
-- Name: screen_playlists screen_playlists_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlists
    ADD CONSTRAINT screen_playlists_pkey PRIMARY KEY (id);


--
-- Name: submission_outbox submission_outbox_judgement_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submission_outbox
    ADD CONSTRAINT submission_outbox_judgement_id_key UNIQUE (judgement_id);


--
-- Name: submission_outbox submission_outbox_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submission_outbox
    ADD CONSTRAINT submission_outbox_pkey PRIMARY KEY (id);


--
-- Name: submissions submissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submissions
    ADD CONSTRAINT submissions_pkey PRIMARY KEY (id);


--
-- Name: team_import_batches team_import_batches_batch_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_import_batches
    ADD CONSTRAINT team_import_batches_batch_id_key UNIQUE (batch_id);


--
-- Name: team_import_batches team_import_batches_idempotency_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_import_batches
    ADD CONSTRAINT team_import_batches_idempotency_key_key UNIQUE (idempotency_key);


--
-- Name: team_import_batches team_import_batches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_import_batches
    ADD CONSTRAINT team_import_batches_pkey PRIMARY KEY (id);


--
-- Name: team_members team_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_members
    ADD CONSTRAINT team_members_pkey PRIMARY KEY (id);


--
-- Name: teams teams_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_pkey PRIMARY KEY (id);


--
-- Name: batch_rejudge_items uq_batch_rejudge_items_task_submission; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_items
    ADD CONSTRAINT uq_batch_rejudge_items_task_submission UNIQUE (task_id, submission_id);


--
-- Name: presentation_configs uq_presentation_configs_contest_mode; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presentation_configs
    ADD CONSTRAINT uq_presentation_configs_contest_mode UNIQUE (contest_id, mode);


--
-- Name: user_roles user_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_pkey PRIMARY KEY (user_id, role_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- Name: idx_announcements_contest_pinned; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_announcements_contest_pinned ON public.announcements USING btree (contest_id, pinned);


--
-- Name: idx_announcements_contest_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_announcements_contest_status ON public.announcements USING btree (contest_id, status);


--
-- Name: idx_announcements_scheduled_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_announcements_scheduled_due ON public.announcements USING btree (status, scheduled_at);


--
-- Name: idx_audit_logs_actor_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_audit_logs_actor_user_id ON public.audit_logs USING btree (actor_user_id);


--
-- Name: idx_award_categories_contest_code; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_award_categories_contest_code ON public.award_categories USING btree (contest_id, code);


--
-- Name: idx_award_categories_contest_order; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_award_categories_contest_order ON public.award_categories USING btree (contest_id, display_order);


--
-- Name: idx_award_certificate_rows_contest; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_award_certificate_rows_contest ON public.award_certificate_rows USING btree (contest_id, export_order);


--
-- Name: idx_award_host_script_sections_script; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_award_host_script_sections_script ON public.award_host_script_sections USING btree (host_script_id, display_order);


--
-- Name: idx_award_recipients_contest_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_award_recipients_contest_category ON public.award_recipients USING btree (contest_id, category_id);


--
-- Name: idx_award_recipients_contest_category_team_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_award_recipients_contest_category_team_key ON public.award_recipients USING btree (contest_id, category_id, team_id, award_key);


--
-- Name: idx_award_rules_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_award_rules_category ON public.award_rules USING btree (category_id);


--
-- Name: idx_balloon_tasks_contest_first_blood; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_balloon_tasks_contest_first_blood ON public.balloon_tasks USING btree (contest_id, is_first_blood);


--
-- Name: idx_balloon_tasks_contest_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_balloon_tasks_contest_status ON public.balloon_tasks USING btree (contest_id, status);


--
-- Name: idx_balloon_tasks_contest_team_problem; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_balloon_tasks_contest_team_problem ON public.balloon_tasks USING btree (contest_id, team_id, problem_id);


--
-- Name: idx_batch_rejudge_items_task_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_batch_rejudge_items_task_status ON public.batch_rejudge_items USING btree (task_id, status);


--
-- Name: idx_batch_rejudge_tasks_contest_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_batch_rejudge_tasks_contest_created ON public.batch_rejudge_tasks USING btree (contest_id, created_at DESC);


--
-- Name: idx_broadcast_tokens_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_broadcast_tokens_active ON public.broadcast_tokens USING btree (contest_id, revoked_at, expires_at);


--
-- Name: idx_broadcast_tokens_contest; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_broadcast_tokens_contest ON public.broadcast_tokens USING btree (contest_id, created_at DESC);


--
-- Name: idx_clarifications_contest_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_clarifications_contest_status ON public.clarifications USING btree (contest_id, status, created_at DESC);


--
-- Name: idx_clarifications_contest_team_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_clarifications_contest_team_created ON public.clarifications USING btree (contest_id, team_id, created_at DESC);


--
-- Name: idx_contest_admin_assignments_contest_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_admin_assignments_contest_id ON public.contest_admin_assignments USING btree (contest_id);


--
-- Name: idx_contest_admin_assignments_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_admin_assignments_user_id ON public.contest_admin_assignments USING btree (user_id);


--
-- Name: idx_contest_problems_problem_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_problems_problem_id ON public.contest_problems USING btree (problem_id);


--
-- Name: idx_contest_teams_contest_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_teams_contest_id ON public.contest_teams USING btree (contest_id);


--
-- Name: idx_contests_deleted_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contests_deleted_at ON public.contests USING btree (deleted_at);


--
-- Name: idx_contests_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contests_status ON public.contests USING btree (status);


--
-- Name: idx_judgements_submission; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_judgements_submission ON public.judgements USING btree (submission_id);


--
-- Name: idx_judgements_submission_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_judgements_submission_active ON public.judgements USING btree (submission_id, superseded);


--
-- Name: idx_judgements_submission_verdict; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_judgements_submission_verdict ON public.judgements USING btree (submission_id, verdict);


--
-- Name: idx_outbox_status_pending; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_outbox_status_pending ON public.submission_outbox USING btree (status, created_at);


--
-- Name: idx_outbox_submission_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_outbox_submission_status ON public.submission_outbox USING btree (submission_id, status);


--
-- Name: idx_print_requests_contest_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_print_requests_contest_status ON public.print_requests USING btree (contest_id, status);


--
-- Name: idx_print_requests_contest_team_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_print_requests_contest_team_created ON public.print_requests USING btree (contest_id, team_id, created_at DESC);


--
-- Name: idx_problem_attachments_problem_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_problem_attachments_problem_id ON public.problem_attachments USING btree (problem_id);


--
-- Name: idx_problem_statements_lang; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_problem_statements_lang ON public.problem_statements USING btree (lang_code);


--
-- Name: idx_problems_slug_alive; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_problems_slug_alive ON public.problems USING btree (slug);


--
-- Name: idx_resolver_events_run_sequence; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_resolver_events_run_sequence ON public.resolver_events USING btree (run_id, sequence);


--
-- Name: idx_resolver_runs_contest_official; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_resolver_runs_contest_official ON public.resolver_runs USING btree (contest_id, official);


--
-- Name: idx_resolver_team_states_run_team_step; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_resolver_team_states_run_team_step ON public.resolver_team_states USING btree (run_id, team_id, step_index);


--
-- Name: idx_runs_judgement; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_runs_judgement ON public.runs USING btree (judgement_id);


--
-- Name: idx_scoreboard_snapshots_contest_group; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_scoreboard_snapshots_contest_group ON public.scoreboard_snapshots USING btree (contest_id, group_name, generated_at DESC);


--
-- Name: idx_scoreboard_snapshots_contest_variant; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_scoreboard_snapshots_contest_variant ON public.scoreboard_snapshots USING btree (contest_id, variant, generated_at DESC);


--
-- Name: idx_screen_commands_pending; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_commands_pending ON public.screen_commands USING btree (screen_instance_id, acknowledged_at, created_at DESC);


--
-- Name: idx_screen_group_members_group; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_group_members_group ON public.screen_group_members USING btree (group_id);


--
-- Name: idx_screen_groups_contest; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_groups_contest ON public.screen_groups USING btree (contest_id, created_at);


--
-- Name: idx_screen_instances_contest; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_instances_contest ON public.screen_instances USING btree (contest_id, created_at);


--
-- Name: idx_screen_instances_online; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_instances_online ON public.screen_instances USING btree (contest_id, revoked_at, last_seen_at);


--
-- Name: idx_screen_playlist_items_playlist; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_playlist_items_playlist ON public.screen_playlist_items USING btree (playlist_id, display_order);


--
-- Name: idx_screen_playlists_contest; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_screen_playlists_contest ON public.screen_playlists USING btree (contest_id, created_at);


--
-- Name: idx_submissions_contest_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_submissions_contest_status ON public.submissions USING btree (contest_id, status);


--
-- Name: idx_submissions_contest_team; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_submissions_contest_team ON public.submissions USING btree (contest_id, team_id);


--
-- Name: idx_submissions_problem; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_submissions_problem ON public.submissions USING btree (problem_id);


--
-- Name: idx_teams_deleted_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_deleted_at ON public.teams USING btree (deleted_at);


--
-- Name: idx_teams_group_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_teams_group_name ON public.teams USING btree (group_name);


--
-- Name: idx_users_user_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_users_user_type ON public.users USING btree (user_type);


--
-- Name: uq_judgements_one_active_per_submission; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_judgements_one_active_per_submission ON public.judgements USING btree (submission_id, active_marker);


--
-- Name: announcements announcements_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.announcements
    ADD CONSTRAINT announcements_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: award_categories award_categories_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_categories
    ADD CONSTRAINT award_categories_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: award_certificate_rows award_certificate_rows_award_recipient_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows
    ADD CONSTRAINT award_certificate_rows_award_recipient_id_fkey FOREIGN KEY (award_recipient_id) REFERENCES public.award_recipients(id) ON DELETE CASCADE;


--
-- Name: award_certificate_rows award_certificate_rows_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_certificate_rows
    ADD CONSTRAINT award_certificate_rows_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: award_host_script_sections award_host_script_sections_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections
    ADD CONSTRAINT award_host_script_sections_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.award_categories(id) ON DELETE CASCADE;


--
-- Name: award_host_script_sections award_host_script_sections_host_script_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_script_sections
    ADD CONSTRAINT award_host_script_sections_host_script_id_fkey FOREIGN KEY (host_script_id) REFERENCES public.award_host_scripts(id) ON DELETE CASCADE;


--
-- Name: award_host_scripts award_host_scripts_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_scripts
    ADD CONSTRAINT award_host_scripts_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: award_host_scripts award_host_scripts_updated_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_host_scripts
    ADD CONSTRAINT award_host_scripts_updated_by_user_id_fkey FOREIGN KEY (updated_by_user_id) REFERENCES public.users(id);


--
-- Name: award_presentation_states award_presentation_states_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states
    ADD CONSTRAINT award_presentation_states_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: award_presentation_states award_presentation_states_current_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states
    ADD CONSTRAINT award_presentation_states_current_category_id_fkey FOREIGN KEY (current_category_id) REFERENCES public.award_categories(id) ON DELETE SET NULL;


--
-- Name: award_presentation_states award_presentation_states_updated_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_presentation_states
    ADD CONSTRAINT award_presentation_states_updated_by_user_id_fkey FOREIGN KEY (updated_by_user_id) REFERENCES public.users(id);


--
-- Name: award_recipients award_recipients_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients
    ADD CONSTRAINT award_recipients_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.award_categories(id);


--
-- Name: award_recipients award_recipients_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients
    ADD CONSTRAINT award_recipients_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: award_recipients award_recipients_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients
    ADD CONSTRAINT award_recipients_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id);


--
-- Name: award_recipients award_recipients_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_recipients
    ADD CONSTRAINT award_recipients_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id);


--
-- Name: award_rules award_rules_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.award_rules
    ADD CONSTRAINT award_rules_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.award_categories(id) ON DELETE CASCADE;


--
-- Name: balloon_tasks balloon_tasks_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks
    ADD CONSTRAINT balloon_tasks_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: balloon_tasks balloon_tasks_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks
    ADD CONSTRAINT balloon_tasks_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id);


--
-- Name: balloon_tasks balloon_tasks_submission_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks
    ADD CONSTRAINT balloon_tasks_submission_id_fkey FOREIGN KEY (submission_id) REFERENCES public.submissions(id);


--
-- Name: balloon_tasks balloon_tasks_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.balloon_tasks
    ADD CONSTRAINT balloon_tasks_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id);


--
-- Name: batch_rejudge_items batch_rejudge_items_submission_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_items
    ADD CONSTRAINT batch_rejudge_items_submission_id_fkey FOREIGN KEY (submission_id) REFERENCES public.submissions(id);


--
-- Name: batch_rejudge_items batch_rejudge_items_task_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_items
    ADD CONSTRAINT batch_rejudge_items_task_id_fkey FOREIGN KEY (task_id) REFERENCES public.batch_rejudge_tasks(id) ON DELETE CASCADE;


--
-- Name: batch_rejudge_tasks batch_rejudge_tasks_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_tasks
    ADD CONSTRAINT batch_rejudge_tasks_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: batch_rejudge_tasks batch_rejudge_tasks_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_rejudge_tasks
    ADD CONSTRAINT batch_rejudge_tasks_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: broadcast_tokens broadcast_tokens_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.broadcast_tokens
    ADD CONSTRAINT broadcast_tokens_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: broadcast_tokens broadcast_tokens_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.broadcast_tokens
    ADD CONSTRAINT broadcast_tokens_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: clarifications clarifications_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.clarifications
    ADD CONSTRAINT clarifications_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: clarifications clarifications_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.clarifications
    ADD CONSTRAINT clarifications_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id);


--
-- Name: clarifications clarifications_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.clarifications
    ADD CONSTRAINT clarifications_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id);


--
-- Name: contest_admin_assignments contest_admin_assignments_assigned_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments
    ADD CONSTRAINT contest_admin_assignments_assigned_by_user_id_fkey FOREIGN KEY (assigned_by_user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: contest_admin_assignments contest_admin_assignments_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments
    ADD CONSTRAINT contest_admin_assignments_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: contest_admin_assignments contest_admin_assignments_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_admin_assignments
    ADD CONSTRAINT contest_admin_assignments_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: contest_problems contest_problems_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_problems
    ADD CONSTRAINT contest_problems_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: contest_problems contest_problems_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_problems
    ADD CONSTRAINT contest_problems_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id) ON DELETE RESTRICT;


--
-- Name: contest_teams contest_teams_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams
    ADD CONSTRAINT contest_teams_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: contest_teams contest_teams_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams
    ADD CONSTRAINT contest_teams_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id) ON DELETE CASCADE;


--
-- Name: judgements judgements_submission_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.judgements
    ADD CONSTRAINT judgements_submission_id_fkey FOREIGN KEY (submission_id) REFERENCES public.submissions(id);


--
-- Name: presentation_configs presentation_configs_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presentation_configs
    ADD CONSTRAINT presentation_configs_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: presentation_configs presentation_configs_updated_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presentation_configs
    ADD CONSTRAINT presentation_configs_updated_by_user_id_fkey FOREIGN KEY (updated_by_user_id) REFERENCES public.users(id);


--
-- Name: print_requests print_requests_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.print_requests
    ADD CONSTRAINT print_requests_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: print_requests print_requests_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.print_requests
    ADD CONSTRAINT print_requests_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id);


--
-- Name: problem_attachments problem_attachments_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problem_attachments
    ADD CONSTRAINT problem_attachments_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id) ON DELETE CASCADE;


--
-- Name: problem_statements problem_statements_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.problem_statements
    ADD CONSTRAINT problem_statements_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id) ON DELETE CASCADE;


--
-- Name: resolver_current_state resolver_current_state_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_current_state
    ADD CONSTRAINT resolver_current_state_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.resolver_runs(id);


--
-- Name: resolver_events resolver_events_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_events
    ADD CONSTRAINT resolver_events_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.resolver_runs(id);


--
-- Name: resolver_runs resolver_runs_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_runs
    ADD CONSTRAINT resolver_runs_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: resolver_snapshots resolver_snapshots_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_snapshots
    ADD CONSTRAINT resolver_snapshots_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.resolver_runs(id);


--
-- Name: resolver_team_states resolver_team_states_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.resolver_team_states
    ADD CONSTRAINT resolver_team_states_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.resolver_runs(id);


--
-- Name: role_permissions role_permissions_permission_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions
    ADD CONSTRAINT role_permissions_permission_id_fkey FOREIGN KEY (permission_id) REFERENCES public.permissions(id) ON DELETE CASCADE;


--
-- Name: role_permissions role_permissions_role_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions
    ADD CONSTRAINT role_permissions_role_id_fkey FOREIGN KEY (role_id) REFERENCES public.roles(id) ON DELETE CASCADE;


--
-- Name: runs runs_judgement_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runs
    ADD CONSTRAINT runs_judgement_id_fkey FOREIGN KEY (judgement_id) REFERENCES public.judgements(id);


--
-- Name: scoreboard_snapshots scoreboard_snapshots_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.scoreboard_snapshots
    ADD CONSTRAINT scoreboard_snapshots_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: screen_commands screen_commands_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_commands
    ADD CONSTRAINT screen_commands_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: screen_commands screen_commands_screen_instance_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_commands
    ADD CONSTRAINT screen_commands_screen_instance_id_fkey FOREIGN KEY (screen_instance_id) REFERENCES public.screen_instances(id) ON DELETE CASCADE;


--
-- Name: screen_group_members screen_group_members_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_group_members
    ADD CONSTRAINT screen_group_members_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.screen_groups(id) ON DELETE CASCADE;


--
-- Name: screen_group_members screen_group_members_screen_instance_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_group_members
    ADD CONSTRAINT screen_group_members_screen_instance_id_fkey FOREIGN KEY (screen_instance_id) REFERENCES public.screen_instances(id) ON DELETE CASCADE;


--
-- Name: screen_groups screen_groups_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups
    ADD CONSTRAINT screen_groups_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: screen_groups screen_groups_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups
    ADD CONSTRAINT screen_groups_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: screen_groups screen_groups_playlist_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_groups
    ADD CONSTRAINT screen_groups_playlist_id_fkey FOREIGN KEY (playlist_id) REFERENCES public.screen_playlists(id);


--
-- Name: screen_instances screen_instances_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_instances
    ADD CONSTRAINT screen_instances_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: screen_playlist_items screen_playlist_items_playlist_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlist_items
    ADD CONSTRAINT screen_playlist_items_playlist_id_fkey FOREIGN KEY (playlist_id) REFERENCES public.screen_playlists(id) ON DELETE CASCADE;


--
-- Name: screen_playlists screen_playlists_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlists
    ADD CONSTRAINT screen_playlists_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: screen_playlists screen_playlists_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.screen_playlists
    ADD CONSTRAINT screen_playlists_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: submission_outbox submission_outbox_judgement_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submission_outbox
    ADD CONSTRAINT submission_outbox_judgement_id_fkey FOREIGN KEY (judgement_id) REFERENCES public.judgements(id);


--
-- Name: submission_outbox submission_outbox_submission_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submission_outbox
    ADD CONSTRAINT submission_outbox_submission_id_fkey FOREIGN KEY (submission_id) REFERENCES public.submissions(id);


--
-- Name: submissions submissions_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submissions
    ADD CONSTRAINT submissions_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: submissions submissions_problem_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submissions
    ADD CONSTRAINT submissions_problem_id_fkey FOREIGN KEY (problem_id) REFERENCES public.problems(id);


--
-- Name: submissions submissions_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.submissions
    ADD CONSTRAINT submissions_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id);


--
-- Name: team_import_batches team_import_batches_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_import_batches
    ADD CONSTRAINT team_import_batches_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: team_members team_members_team_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.team_members
    ADD CONSTRAINT team_members_team_id_fkey FOREIGN KEY (team_id) REFERENCES public.teams(id) ON DELETE CASCADE;


--
-- Name: user_roles user_roles_role_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_role_id_fkey FOREIGN KEY (role_id) REFERENCES public.roles(id) ON DELETE CASCADE;


--
-- Name: user_roles user_roles_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_roles
    ADD CONSTRAINT user_roles_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

INSERT INTO public.roles (code, name, description, builtin) VALUES
    ('CONTEST_ADMIN', 'Contest Administrator', 'Manages contests and teams', true),
    ('TEAM_LEADER', 'Team Leader', 'Team-side read-only role', true),
    ('BALLOON_STAFF', 'Balloon Staff', 'On-site balloon delivery operator', true),
    ('AWARD_OPERATOR', 'Award Operator', 'Manages award categories, recipients, and freeze', true),
    ('JUDGE', 'Judge', 'Replies to clarifications and processes submissions', true),
    ('PRINTER', 'Printer', 'On-site print queue operator', true),
    ('RESOLVER_OPERATOR', 'Resolver Operator', 'Operates official scoreboard resolver runs', true),
    ('SCREEN_OPERATOR', 'Screen Operator', 'Configures venue presentation screens', true),
    ('LIVE_OPERATOR', 'Live Operator', 'Configures public broadcast presentation pages', true);
