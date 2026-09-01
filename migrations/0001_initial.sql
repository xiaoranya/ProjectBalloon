-- ProjectBalloon consolidated alpha schema.
--
-- Single-file SQLx migration for fresh installations. It was produced by
-- merging the original timestamped migration history in application order;
-- each section banner names the source migration for traceability.
--
-- Alpha note: this consolidation is destructive. Existing alpha databases
-- must be dropped and recreated (or restored from a backup) before running
-- this migration; in-place upgrades from the previous migration chain are
-- not supported.

-- ============================================================================
-- Source migration (01/50): 20260719000000_initial_baseline.sql
-- ============================================================================

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
-- Name: contest_management_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contest_management_assignments (
    id bigint NOT NULL,
    user_id bigint NOT NULL,
    contest_id bigint NOT NULL,
    assigned_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contest_management_assignments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.contest_management_assignments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: contest_management_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.contest_management_assignments_id_seq OWNED BY public.contest_management_assignments.id;


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
-- Name: live_programs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.live_programs (
    id bigint NOT NULL,
    contest_id bigint NOT NULL,
    current_scene character varying(32) DEFAULT 'SCOREBOARD'::character varying NOT NULL,
    resolver_run_id bigint,
    transition_milliseconds integer DEFAULT 800 NOT NULL,
    show_clock boolean DEFAULT true NOT NULL,
    ticker_enabled boolean DEFAULT true NOT NULL,
    title_card_text character varying(240),
    updated_by_user_id bigint,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT ck_live_programs_transition CHECK (((transition_milliseconds >= 100) AND (transition_milliseconds <= 5000))),
    CONSTRAINT ck_live_programs_scene CHECK ((current_scene IN ('SCOREBOARD', 'FIRST_BLOOD', 'BALLOONS', 'FREEZE_COUNTDOWN', 'STATISTICS', 'RESOLVER', 'AWARDS', 'TITLE_CARD')))
);


--
-- Name: live_programs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.live_programs_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: live_programs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.live_programs_id_seq OWNED BY public.live_programs.id;


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
-- Name: contest_management_assignments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments ALTER COLUMN id SET DEFAULT nextval('public.contest_management_assignments_id_seq'::regclass);


--
-- Name: contest_teams id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_teams ALTER COLUMN id SET DEFAULT nextval('public.contest_teams_id_seq'::regclass);


--
-- Name: contests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contests ALTER COLUMN id SET DEFAULT nextval('public.contests_id_seq'::regclass);


--
-- Name: live_programs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs ALTER COLUMN id SET DEFAULT nextval('public.live_programs_id_seq'::regclass);


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
-- Name: contest_management_assignments contest_management_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments
    ADD CONSTRAINT contest_management_assignments_pkey PRIMARY KEY (id);


--
-- Name: contest_management_assignments contest_management_assignments_user_id_contest_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments
    ADD CONSTRAINT contest_management_assignments_user_id_contest_id_key UNIQUE (user_id, contest_id);


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
-- Name: live_programs live_programs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs
    ADD CONSTRAINT live_programs_pkey PRIMARY KEY (id);


--
-- Name: live_programs live_programs_contest_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs
    ADD CONSTRAINT live_programs_contest_id_key UNIQUE (contest_id);


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
-- Name: idx_contest_management_assignments_contest_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_management_assignments_contest_id ON public.contest_management_assignments USING btree (contest_id);


--
-- Name: idx_contest_management_assignments_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_contest_management_assignments_user_id ON public.contest_management_assignments USING btree (user_id);


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
-- Name: contest_management_assignments contest_management_assignments_assigned_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments
    ADD CONSTRAINT contest_management_assignments_assigned_by_user_id_fkey FOREIGN KEY (assigned_by_user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: contest_management_assignments contest_management_assignments_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments
    ADD CONSTRAINT contest_management_assignments_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id) ON DELETE CASCADE;


--
-- Name: contest_management_assignments contest_management_assignments_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contest_management_assignments
    ADD CONSTRAINT contest_management_assignments_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


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
-- Name: live_programs live_programs_contest_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs
    ADD CONSTRAINT live_programs_contest_id_fkey FOREIGN KEY (contest_id) REFERENCES public.contests(id);


--
-- Name: live_programs live_programs_resolver_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs
    ADD CONSTRAINT live_programs_resolver_run_id_fkey FOREIGN KEY (resolver_run_id) REFERENCES public.resolver_runs(id);


--
-- Name: live_programs live_programs_updated_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.live_programs
    ADD CONSTRAINT live_programs_updated_by_user_id_fkey FOREIGN KEY (updated_by_user_id) REFERENCES public.users(id);


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
-- PostgreSQL database dump complete
--

-- ============================================================================
-- Source migration (02/50): 20260719010000_auth_sessions.sql
-- ============================================================================

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

-- ============================================================================
-- Source migration (03/50): 20260719020000_direct_user_permissions.sql
-- ============================================================================

INSERT INTO permissions (code, name, description) VALUES
    ('CONTEST_MANAGE', 'Contest management', 'Manage assigned contests, teams, problems, submissions, announcements, scoring, and scoreboards'),
    ('CLARIFICATION_MANAGE', 'Clarification management', 'Review and answer contest clarifications'),
    ('PRINTING_MANAGE', 'Printing management', 'Operate the contest print queue'),
    ('BALLOON_MANAGE', 'Balloon management', 'Operate balloon dispatch and delivery'),
    ('RESOLVER_MANAGE', 'Resolver management', 'Operate official scoreboard resolver runs'),
    ('AWARD_MANAGE', 'Award management', 'Manage awards and award presentation'),
    ('SCREEN_MANAGE', 'Screen management', 'Configure venue presentation screens'),
    ('LIVE_MANAGE', 'Live management', 'Configure public broadcast presentation pages');

CREATE TABLE user_permissions (
    user_id bigint NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id bigint NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, permission_id)
);

-- ============================================================================
-- Source migration (04/50): 20260719030000_admin_query_indexes.sql
-- ============================================================================

-- Default audit browsing is reverse chronological. Staff and scope screens
-- filter by user type and then order by username.
CREATE INDEX idx_audit_logs_created_at
    ON audit_logs (created_at DESC, id DESC);

CREATE INDEX idx_users_user_type_username
    ON users (user_type, username, id);

-- ============================================================================
-- Source migration (05/50): 20260719040000_contest_integrity.sql
-- ============================================================================

-- Application-level duplicate checks are race-prone. Preserve the legacy
-- ability to reuse a soft-deleted contest name while enforcing active-name
-- uniqueness in PostgreSQL.
CREATE UNIQUE INDEX idx_contests_active_name_unique
    ON contests (name)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_contests_updated_at
    ON contests (updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_contests_start_at
    ON contests (start_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- ============================================================================
-- Source migration (06/50): 20260719050000_realtime_outbox.sql
-- ============================================================================

-- Business transactions persist realtime notifications before commit. A
-- dispatcher introduced with the realtime slice will publish pending rows to
-- Redis/SSE with retry and mark them delivered.
CREATE TABLE realtime_outbox (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id uuid NOT NULL UNIQUE,
    contest_id bigint NOT NULL REFERENCES contests(id),
    event_type varchar(64) NOT NULL,
    schema_version smallint NOT NULL DEFAULT 1,
    scope varchar(16) NOT NULL,
    payload_json jsonb NOT NULL,
    status varchar(16) NOT NULL DEFAULT 'PENDING',
    attempts integer NOT NULL DEFAULT 0,
    available_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    published_at timestamptz,
    last_error text,
    CONSTRAINT realtime_outbox_scope_check
        CHECK (scope IN ('PUBLIC', 'STAFF', 'TEAM')),
    CONSTRAINT realtime_outbox_schema_version_check
        CHECK (schema_version > 0),
    CONSTRAINT realtime_outbox_status_check
        CHECK (status IN ('PENDING', 'PUBLISHING', 'PUBLISHED', 'FAILED')),
    CONSTRAINT realtime_outbox_attempts_check
        CHECK (attempts >= 0)
);

CREATE INDEX idx_realtime_outbox_pending
    ON realtime_outbox (available_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_realtime_outbox_contest_created
    ON realtime_outbox (contest_id, created_at DESC);

-- ============================================================================
-- Source migration (07/50): 20260719060000_realtime_team_scope.sql
-- ============================================================================

-- TEAM events must name their private recipient. PUBLIC and STAFF events must
-- not carry a team identifier, which prevents accidental cross-scope fanout.
ALTER TABLE realtime_outbox
    ADD COLUMN team_id bigint REFERENCES teams(id);

ALTER TABLE realtime_outbox
    ADD CONSTRAINT realtime_outbox_recipient_check
    CHECK (
        (scope = 'TEAM' AND team_id IS NOT NULL)
        OR (scope IN ('PUBLIC', 'STAFF') AND team_id IS NULL)
    );

-- ============================================================================
-- Source migration (08/50): 20260719070000_team_accounts.sql
-- ============================================================================

-- Team authentication must use an immutable identifier, never a display-name
-- convention. One login belongs to exactly one team and one team has at most
-- one primary contestant login in the first Rust version.
CREATE TABLE team_accounts (
    user_id bigint PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    team_id bigint NOT NULL UNIQUE REFERENCES teams(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE teams
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT teams_version_check CHECK (version >= 0);

CREATE UNIQUE INDEX idx_teams_active_name_unique
    ON teams (lower(name))
    WHERE deleted_at IS NULL;

ALTER TABLE contest_teams
    ADD CONSTRAINT contest_teams_participation_type_check
    CHECK (participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE'));

CREATE INDEX idx_contest_teams_team_id
    ON contest_teams (team_id, contest_id);

CREATE INDEX idx_team_members_team_created
    ON team_members (team_id, created_at, id);

-- ============================================================================
-- Source migration (09/50): 20260719080000_problem_integrity.sql
-- ============================================================================

ALTER TABLE problems DROP CONSTRAINT problems_slug_key;
DROP INDEX idx_problems_slug_alive;

CREATE UNIQUE INDEX idx_problems_active_slug_unique
    ON problems (slug)
    WHERE deleted_at IS NULL;

ALTER TABLE problems
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT problems_time_limit_positive CHECK (time_limit_ms > 0),
    ADD CONSTRAINT problems_memory_limit_positive CHECK (memory_limit_mb > 0),
    ADD CONSTRAINT problems_output_limit_positive CHECK (output_limit_kb > 0),
    ADD CONSTRAINT problems_testdata_version_nonnegative CHECK (testdata_version >= 0),
    ADD CONSTRAINT problems_version_nonnegative CHECK (version >= 0);

-- ============================================================================
-- Source migration (10/50): 20260719090000_contest_problem_integrity.sql
-- ============================================================================

ALTER TABLE contest_problems
    DROP CONSTRAINT contest_problems_contest_id_display_order_key,
    ADD CONSTRAINT contest_problems_contest_id_display_order_key
        UNIQUE (contest_id, display_order)
        DEFERRABLE INITIALLY IMMEDIATE,
    ADD CONSTRAINT contest_problems_alias_format_check
        CHECK (alias ~ '^[A-Z0-9]{1,8}$'),
    ADD CONSTRAINT contest_problems_display_order_check
        CHECK (display_order BETWEEN 1 AND 1000);

-- ============================================================================
-- Source migration (11/50): 20260719100000_problem_testdata_versions.sql
-- ============================================================================

CREATE TABLE problem_testdata_versions (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    problem_id bigint NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    version integer NOT NULL,
    object_key varchar(512) NOT NULL,
    sha256 varchar(64) NOT NULL,
    bytes bigint,
    uploaded_by_user_id bigint REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT problem_testdata_versions_version_positive CHECK (version > 0),
    CONSTRAINT problem_testdata_versions_bytes_positive CHECK (bytes IS NULL OR bytes > 0),
    CONSTRAINT problem_testdata_versions_sha256_format
        CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT problem_testdata_versions_problem_version_unique UNIQUE (problem_id, version),
    CONSTRAINT problem_testdata_versions_object_key_unique UNIQUE (object_key)
);

CREATE INDEX idx_problem_testdata_versions_problem_created
    ON problem_testdata_versions (problem_id, version DESC);

INSERT INTO problem_testdata_versions
    (problem_id, version, object_key, sha256, bytes, uploaded_by_user_id)
SELECT id, testdata_version, testdata_object_key, testdata_sha256, NULL, created_by
FROM problems
WHERE testdata_version > 0
  AND testdata_object_key IS NOT NULL
  AND testdata_sha256 ~ '^[0-9a-f]{64}$'
ON CONFLICT (problem_id, version) DO NOTHING;

COMMENT ON TABLE problem_testdata_versions IS
    'Immutable ready test-data archives. Object bytes are never replaced for an existing version.';

-- ============================================================================
-- Source migration (12/50): 20260719110000_testdata_case_count.sql
-- ============================================================================

ALTER TABLE problem_testdata_versions
    ADD COLUMN case_count integer,
    ADD CONSTRAINT problem_testdata_versions_case_count_positive
        CHECK (case_count IS NULL OR case_count > 0);

COMMENT ON COLUMN problem_testdata_versions.case_count IS
    'Validated number of root-level .in/.out pairs; NULL only for bridged legacy versions.';

-- ============================================================================
-- Source migration (13/50): 20260719120000_submission_integrity.sql
-- ============================================================================

ALTER TABLE submissions
    ADD COLUMN source_sha256 varchar(64),
    ADD CONSTRAINT submissions_source_size_bounds
        CHECK (source_size_bytes BETWEEN 1 AND 65536),
    ADD CONSTRAINT submissions_source_sha256_format
        CHECK (source_sha256 IS NULL OR source_sha256 ~ '^[0-9a-f]{64}$');

ALTER TABLE submission_outbox
    ADD CONSTRAINT submission_outbox_attempts_nonnegative CHECK (attempts >= 0),
    ADD CONSTRAINT submission_outbox_status_known
        CHECK (status IN ('PENDING', 'PUBLISHING', 'SENT', 'FAILED'));

CREATE INDEX idx_submissions_team_recent
    ON submissions (team_id, submitted_at DESC, id DESC);

COMMENT ON COLUMN submissions.source_sha256 IS
    'SHA-256 of the exact source bytes dispatched to Judge; NULL only for bridged legacy rows.';

-- ============================================================================
-- Source migration (14/50): 20260719130000_submission_outbox_lease.sql
-- ============================================================================

ALTER TABLE submission_outbox
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN lease_owner uuid,
    ADD COLUMN lease_until timestamptz,
    ADD CONSTRAINT submission_outbox_lease_shape CHECK (
        (status = 'PUBLISHING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PUBLISHING' AND lease_owner IS NULL AND lease_until IS NULL)
    );

DROP INDEX idx_outbox_status_pending;

CREATE INDEX idx_submission_outbox_dispatchable
    ON submission_outbox (available_at, created_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_submission_outbox_expired_lease
    ON submission_outbox (lease_until, id)
    WHERE status = 'PUBLISHING';

-- ============================================================================
-- Source migration (15/50): 20260719140000_judge_results.sql
-- ============================================================================

ALTER TABLE judgements
    ADD COLUMN result_message_id uuid;

CREATE UNIQUE INDEX uq_judgements_result_message_id
    ON judgements (result_message_id)
    WHERE result_message_id IS NOT NULL;

CREATE UNIQUE INDEX uq_runs_judgement_test_index
    ON runs (judgement_id, test_index);

-- ============================================================================
-- Source migration (16/50): 20260719150000_judge_workers.sql
-- ============================================================================

CREATE TABLE judge_workers (
    worker_id varchar(64) PRIMARY KEY,
    instance_id uuid NOT NULL,
    started_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    capacity smallint NOT NULL,
    active_tasks smallint NOT NULL,
    languages jsonb NOT NULL,
    runtime_versions jsonb NOT NULL,
    sandbox_runtime varchar(64),
    last_message_id uuid NOT NULL UNIQUE,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT judge_workers_capacity_check
        CHECK (capacity > 0 AND active_tasks BETWEEN 0 AND capacity),
    CONSTRAINT judge_workers_languages_array_check
        CHECK (jsonb_typeof(languages) = 'array'),
    CONSTRAINT judge_workers_runtime_versions_object_check
        CHECK (jsonb_typeof(runtime_versions) = 'object')
);

CREATE INDEX idx_judge_workers_last_seen
    ON judge_workers (last_seen_at DESC, worker_id);

-- ============================================================================
-- Source migration (17/50): 20260719160000_icpc_scoreboard_projection.sql
-- ============================================================================

CREATE TABLE contest_scoreboard_cells (
    contest_id bigint NOT NULL REFERENCES contests(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    problem_id bigint NOT NULL REFERENCES problems(id),
    wrong_attempts integer NOT NULL DEFAULT 0,
    solved boolean NOT NULL DEFAULT false,
    solved_at timestamptz,
    first_accepted_submission_id bigint REFERENCES submissions(id),
    penalty_minutes bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (contest_id, team_id, problem_id),
    CONSTRAINT scoreboard_cell_wrong_attempts_check CHECK (wrong_attempts >= 0),
    CONSTRAINT scoreboard_cell_penalty_check CHECK (penalty_minutes >= 0),
    CONSTRAINT scoreboard_cell_solved_state_check CHECK (
        (solved AND solved_at IS NOT NULL AND first_accepted_submission_id IS NOT NULL)
        OR
        (NOT solved AND solved_at IS NULL AND first_accepted_submission_id IS NULL
            AND penalty_minutes = 0)
    )
);

CREATE TABLE contest_scoreboard_rows (
    contest_id bigint NOT NULL REFERENCES contests(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    solved_count integer NOT NULL DEFAULT 0,
    penalty_minutes bigint NOT NULL DEFAULT 0,
    last_solved_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (contest_id, team_id),
    CONSTRAINT scoreboard_row_solved_count_check CHECK (solved_count >= 0),
    CONSTRAINT scoreboard_row_penalty_check CHECK (penalty_minutes >= 0),
    CONSTRAINT scoreboard_row_last_solve_check CHECK (
        (solved_count = 0 AND last_solved_at IS NULL)
        OR (solved_count > 0 AND last_solved_at IS NOT NULL)
    )
);

CREATE INDEX idx_scoreboard_rows_ranking
    ON contest_scoreboard_rows
        (contest_id, solved_count DESC, penalty_minutes, last_solved_at, team_id);

CREATE INDEX idx_scoreboard_cells_problem_solved
    ON contest_scoreboard_cells (contest_id, problem_id, solved_at, team_id)
    WHERE solved;

-- ============================================================================
-- Source migration (18/50): 20260719170000_scoreboard_snapshot_integrity.sql
-- ============================================================================

ALTER TABLE scoreboard_snapshots
    ADD COLUMN participation_type varchar(32),
    ADD COLUMN payload_sha256 char(64),
    ADD COLUMN created_by_user_id bigint REFERENCES users(id);

ALTER TABLE scoreboard_snapshots
    ADD CONSTRAINT scoreboard_snapshot_participation_type_check
        CHECK (participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')),
    ADD CONSTRAINT scoreboard_snapshot_sha256_check
        CHECK (payload_sha256 IS NULL OR payload_sha256 ~ '^[0-9a-f]{64}$');

CREATE UNIQUE INDEX uq_scoreboard_snapshot_version
    ON scoreboard_snapshots (
        contest_id,
        variant,
        coalesce(group_name, ''),
        coalesce(participation_type, ''),
        version
    );

CREATE OR REPLACE FUNCTION reject_scoreboard_snapshot_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'scoreboard snapshots are immutable';
END;
$$;

CREATE TRIGGER trg_scoreboard_snapshots_immutable
BEFORE UPDATE OR DELETE ON scoreboard_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_scoreboard_snapshot_mutation();

-- ============================================================================
-- Source migration (19/50): 20260719180000_scoreboard_cache_revision.sql
-- ============================================================================

ALTER TABLE contests
    ADD COLUMN scoreboard_revision bigint NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    affected_contest_id bigint;
BEGIN
    affected_contest_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.contest_id ELSE NEW.contest_id END;
    UPDATE contests
    SET scoreboard_revision = scoreboard_revision + 1
    WHERE id = affected_contest_id;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER trg_scoreboard_cell_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_scoreboard_cells
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE TRIGGER trg_contest_roster_scoreboard_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_teams
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE TRIGGER trg_contest_problem_scoreboard_revision
AFTER INSERT OR UPDATE OR DELETE ON contest_problems
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision();

CREATE OR REPLACE FUNCTION bump_contest_scoreboard_revision_for_team()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.name IS DISTINCT FROM NEW.name
       OR OLD.school IS DISTINCT FROM NEW.school
       OR OLD.star IS DISTINCT FROM NEW.star
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        UPDATE contests contest
        SET scoreboard_revision = contest.scoreboard_revision + 1
        WHERE EXISTS (
            SELECT 1
            FROM contest_teams roster
            WHERE roster.contest_id = contest.id
              AND roster.team_id = NEW.id
        );
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_team_scoreboard_revision
AFTER UPDATE OF name, school, star, deleted_at ON teams
FOR EACH ROW EXECUTE FUNCTION bump_contest_scoreboard_revision_for_team();

CREATE OR REPLACE FUNCTION preserve_or_bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status
       OR OLD.start_at IS DISTINCT FROM NEW.start_at
       OR OLD.freeze_at IS DISTINCT FROM NEW.freeze_at
       OR OLD.end_at IS DISTINCT FROM NEW.end_at
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        NEW.scoreboard_revision := OLD.scoreboard_revision + 1;
    ELSE
        NEW.scoreboard_revision := greatest(NEW.scoreboard_revision, OLD.scoreboard_revision);
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_contest_scoreboard_revision
BEFORE UPDATE ON contests
FOR EACH ROW EXECUTE FUNCTION preserve_or_bump_contest_scoreboard_revision();

-- ============================================================================
-- Source migration (20/50): 20260719190000_submission_rejudge.sql
-- ============================================================================

ALTER TABLE submission_outbox
    DROP CONSTRAINT submission_outbox_status_known,
    ADD CONSTRAINT submission_outbox_status_known
        CHECK (status IN ('PENDING', 'PUBLISHING', 'SENT', 'FAILED', 'CANCELLED'));

COMMENT ON COLUMN submission_outbox.status IS
    'CANCELLED is terminal for a task superseded before or during publication by a rejudge.';

-- ============================================================================
-- Source migration (21/50): 20260719200000_batch_rejudge_recovery.sql
-- ============================================================================

ALTER TABLE batch_rejudge_tasks
    ADD COLUMN version bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT batch_rejudge_task_status_known
        CHECK (status IN ('PENDING', 'RUNNING', 'PAUSED', 'COMPLETED', 'CANCELLED')),
    ADD CONSTRAINT batch_rejudge_task_counts_valid
        CHECK (
            total_items >= 0 AND processed_items >= 0 AND succeeded_items >= 0 AND failed_items >= 0
            AND processed_items = succeeded_items + failed_items
            AND processed_items <= total_items
        );

ALTER TABLE batch_rejudge_items
    ADD COLUMN attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN lease_owner uuid,
    ADD COLUMN lease_until timestamptz,
    ADD CONSTRAINT batch_rejudge_item_status_known
        CHECK (status IN ('PENDING', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'CANCELLED')),
    ADD CONSTRAINT batch_rejudge_item_lease_shape
        CHECK (
            (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
            OR
            (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
        );

ALTER TABLE judgements
    ADD COLUMN batch_rejudge_item_id bigint REFERENCES batch_rejudge_items(id);

CREATE UNIQUE INDEX uq_judgements_batch_rejudge_item
    ON judgements (batch_rejudge_item_id)
    WHERE batch_rejudge_item_id IS NOT NULL;

CREATE INDEX idx_batch_rejudge_items_claim
    ON batch_rejudge_items (task_id, created_at, id)
    WHERE status IN ('PENDING', 'PROCESSING');

-- ============================================================================
-- Source migration (22/50): 20260719210000_clarification_integrity.sql
-- ============================================================================

UPDATE clarifications SET status = upper(status);
ALTER TABLE clarifications ALTER COLUMN status SET DEFAULT 'PENDING';

ALTER TABLE clarifications
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD COLUMN closed_by bigint REFERENCES users(id),
    ADD COLUMN closed_at timestamptz,
    ADD CONSTRAINT clarification_scope_known CHECK (scope IN ('GENERAL', 'PROBLEM')),
    ADD CONSTRAINT clarification_scope_problem_shape
        CHECK ((scope = 'GENERAL' AND problem_id IS NULL AND problem_alias IS NULL)
            OR (scope = 'PROBLEM' AND problem_id IS NOT NULL AND problem_alias IS NOT NULL)),
    ADD CONSTRAINT clarification_status_known CHECK (status IN ('PENDING', 'ANSWERED', 'CLOSED')),
    ADD CONSTRAINT clarification_reply_visibility_known
        CHECK (reply_visibility IS NULL OR reply_visibility IN ('PRIVATE', 'PUBLIC')),
    ADD CONSTRAINT clarification_reply_shape
        CHECK ((status = 'PENDING' AND reply IS NULL AND reply_visibility IS NULL
                AND replied_by IS NULL AND replied_at IS NULL)
            OR (status = 'ANSWERED' AND reply IS NOT NULL AND reply_visibility IS NOT NULL
                AND replied_by IS NOT NULL AND replied_at IS NOT NULL)
            OR status = 'CLOSED'),
    ADD CONSTRAINT clarification_closed_shape
        CHECK ((status = 'CLOSED' AND closed_by IS NOT NULL AND closed_at IS NOT NULL)
            OR (status <> 'CLOSED' AND closed_by IS NULL AND closed_at IS NULL)),
    ADD CONSTRAINT clarification_text_bounds
        CHECK (char_length(question) BETWEEN 1 AND 4000
            AND (reply IS NULL OR char_length(reply) BETWEEN 1 AND 8000));

CREATE INDEX idx_clarifications_rate_limit
    ON clarifications (contest_id, team_id, created_at DESC);

-- ============================================================================
-- Source migration (23/50): 20260719220000_announcement_integrity.sql
-- ============================================================================

UPDATE announcements SET status = upper(status);
ALTER TABLE announcements ALTER COLUMN status SET DEFAULT 'PUBLISHED';

ALTER TABLE announcements
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT announcement_status_known
        CHECK (status IN ('PUBLISHED', 'WITHDRAWN', 'SCHEDULED', 'CANCELLED')),
    ADD CONSTRAINT announcement_text_bounds
        CHECK (char_length(title) BETWEEN 1 AND 255 AND char_length(body) BETWEEN 1 AND 16000),
    ADD CONSTRAINT announcement_state_shape CHECK (
        (status = 'PUBLISHED' AND published_at IS NOT NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'WITHDRAWN' AND published_at IS NOT NULL
            AND withdrawn_at IS NOT NULL AND withdrawn_by IS NOT NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'SCHEDULED' AND scheduled_at IS NOT NULL AND published_at IS NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NULL AND cancelled_by IS NULL)
        OR (status = 'CANCELLED' AND scheduled_at IS NOT NULL AND published_at IS NULL
            AND withdrawn_at IS NULL AND withdrawn_by IS NULL
            AND cancelled_at IS NOT NULL AND cancelled_by IS NOT NULL)
    );

CREATE UNIQUE INDEX uq_announcements_source_clarification
    ON announcements (source_clarification_id)
    WHERE source_clarification_id IS NOT NULL;

CREATE INDEX idx_announcements_public_order
    ON announcements (contest_id, pinned DESC, published_at DESC, id DESC)
    WHERE status = 'PUBLISHED';

-- ============================================================================
-- Source migration (24/50): 20260719230000_print_request_integrity.sql
-- ============================================================================

UPDATE print_requests SET failed_reason = NULL
WHERE status NOT IN ('FAILED', 'REJECTED');
UPDATE print_requests SET completed_at = NULL
WHERE status <> 'COMPLETED';

ALTER TABLE print_requests
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT print_request_status_known
        CHECK (status IN ('REQUESTED', 'QUEUED', 'PRINTING', 'COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED')),
    ADD CONSTRAINT print_request_content_bounds
        CHECK (octet_length(content) BETWEEN 1 AND 20480 AND page_count BETWEEN 1 AND 5),
    ADD CONSTRAINT print_request_hash_shape
        CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT print_request_pdf_shape
        CHECK (status = 'REQUESTED' OR (pdf_object_key IS NOT NULL AND pdf_bucket IS NOT NULL)),
    ADD CONSTRAINT print_request_completion_shape
        CHECK ((status = 'COMPLETED' AND completed_at IS NOT NULL)
            OR (status <> 'COMPLETED' AND completed_at IS NULL)),
    ADD CONSTRAINT print_request_failure_shape
        CHECK ((status IN ('FAILED', 'REJECTED') AND failed_reason IS NOT NULL)
            OR (status NOT IN ('FAILED', 'REJECTED') AND failed_reason IS NULL));

CREATE INDEX idx_print_requests_queue_order
    ON print_requests (contest_id, created_at, id)
    WHERE status = 'QUEUED';

-- ============================================================================
-- Source migration (25/50): 20260719240000_cups_delivery_recovery.sql
-- ============================================================================

ALTER TABLE print_requests
    ADD COLUMN delivery_attempts integer NOT NULL DEFAULT 0,
    ADD COLUMN delivery_lease_owner uuid,
    ADD COLUMN delivery_lease_until timestamptz,
    ADD COLUMN submitted_at timestamptz,
    ADD COLUMN cancellation_pending boolean NOT NULL DEFAULT false,
    ADD COLUMN last_delivery_error varchar(255),
    ADD CONSTRAINT print_request_delivery_lease_shape
        CHECK ((delivery_lease_owner IS NULL) = (delivery_lease_until IS NULL)),
    ADD CONSTRAINT print_request_delivery_attempts_valid CHECK (delivery_attempts >= 0),
    ADD CONSTRAINT print_request_cancellation_shape
        CHECK (NOT cancellation_pending OR (status = 'CANCELLED' AND cups_job_id IS NOT NULL));

CREATE INDEX idx_print_requests_delivery_claim
    ON print_requests (status, delivery_lease_until, created_at, id)
    WHERE status IN ('QUEUED', 'PRINTING') OR cancellation_pending;

-- ============================================================================
-- Source migration (26/50): 20260719250000_balloon_task_integrity.sql
-- ============================================================================

UPDATE balloon_tasks SET status = upper(status);
UPDATE balloon_tasks task
SET color = coalesce(nullif(btrim(task.color), ''), nullif(btrim(problem.color), ''), 'UNSET')
FROM contest_problems problem
WHERE problem.contest_id = task.contest_id AND problem.problem_id = task.problem_id;
UPDATE balloon_tasks SET color = 'UNSET' WHERE color IS NULL OR btrim(color) = '';
UPDATE balloon_tasks SET status = 'PENDING' WHERE status NOT IN ('PENDING', 'CLAIMED', 'DELIVERED', 'CANCELLED');
UPDATE balloon_tasks SET status = 'PENDING'
WHERE status IN ('CLAIMED', 'DELIVERED') AND claimed_by IS NULL;
UPDATE balloon_tasks SET claimed_by = NULL, claimed_at = NULL, delivered_at = NULL,
    cancelled_at = NULL, cancelled_reason = NULL WHERE status = 'PENDING';
UPDATE balloon_tasks SET claimed_at = coalesce(claimed_at, updated_at), delivered_at = NULL,
    cancelled_at = NULL, cancelled_reason = NULL WHERE status = 'CLAIMED';
UPDATE balloon_tasks SET claimed_at = coalesce(claimed_at, updated_at),
    delivered_at = coalesce(delivered_at, updated_at), cancelled_at = NULL,
    cancelled_reason = NULL WHERE status = 'DELIVERED';
UPDATE balloon_tasks SET delivered_at = NULL, cancelled_at = coalesce(cancelled_at, updated_at),
    cancelled_reason = coalesce(nullif(btrim(cancelled_reason), ''), 'legacy cancellation'),
    claimed_at = CASE WHEN claimed_by IS NULL THEN NULL ELSE coalesce(claimed_at, updated_at) END
WHERE status = 'CANCELLED';

WITH ranked AS (
    SELECT task.id, row_number() OVER (
        PARTITION BY task.contest_id, task.problem_id
        ORDER BY submission.submitted_at, task.team_id, task.submission_id, task.id
    ) AS position
    FROM balloon_tasks task
    JOIN submissions submission ON submission.id = task.submission_id
    WHERE task.is_first_blood
)
UPDATE balloon_tasks task SET is_first_blood = false
FROM ranked WHERE task.id = ranked.id AND ranked.position > 1;

ALTER TABLE balloon_tasks
    ALTER COLUMN status SET DEFAULT 'PENDING',
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD COLUMN reopened_count integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT balloon_task_status_known
        CHECK (status IN ('PENDING', 'CLAIMED', 'DELIVERED', 'CANCELLED')),
    ADD CONSTRAINT balloon_task_color_present
        CHECK (color IS NOT NULL AND char_length(btrim(color)) BETWEEN 1 AND 16),
    ADD CONSTRAINT balloon_task_note_bounds
        CHECK (note IS NULL OR char_length(note) <= 2000),
    ADD CONSTRAINT balloon_task_reopened_count_valid CHECK (reopened_count >= 0),
    ADD CONSTRAINT balloon_task_state_shape CHECK (
        (status = 'PENDING' AND claimed_by IS NULL AND claimed_at IS NULL
            AND delivered_at IS NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'CLAIMED' AND claimed_by IS NOT NULL AND claimed_at IS NOT NULL
            AND delivered_at IS NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'DELIVERED' AND claimed_by IS NOT NULL AND claimed_at IS NOT NULL
            AND delivered_at IS NOT NULL AND cancelled_at IS NULL AND cancelled_reason IS NULL)
        OR (status = 'CANCELLED' AND delivered_at IS NULL AND cancelled_at IS NOT NULL
            AND cancelled_reason IS NOT NULL
            AND ((claimed_by IS NULL AND claimed_at IS NULL)
                OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL)))
    );

CREATE UNIQUE INDEX idx_balloon_tasks_first_blood_unique
    ON balloon_tasks (contest_id, problem_id) WHERE is_first_blood;

CREATE INDEX idx_balloon_tasks_workbench
    ON balloon_tasks (contest_id, status, is_first_blood DESC, created_at, id);

-- ============================================================================
-- Source migration (27/50): 20260719260000_resolver_integrity.sql
-- ============================================================================

UPDATE resolver_runs SET status = upper(status);
UPDATE resolver_runs SET status = 'READY'
WHERE status NOT IN ('READY', 'RUNNING', 'PAUSED', 'COMPLETED');
UPDATE resolver_runs SET current_step = greatest(current_step, 0),
    total_steps = greatest(total_steps, current_step, 0);

ALTER TABLE resolver_runs
    ADD COLUMN source_public_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN source_final_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN plan_sha256 char(64) NOT NULL DEFAULT repeat('0', 64),
    ADD COLUMN created_by_user_id bigint REFERENCES users(id),
    ADD COLUMN started_at timestamptz,
    ADD COLUMN completed_at timestamptz,
    ADD COLUMN auto_play_enabled boolean NOT NULL DEFAULT false,
    ADD COLUMN auto_play_interval_ms integer NOT NULL DEFAULT 3000,
    ADD COLUMN next_auto_at timestamptz,
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT resolver_run_status_known
        CHECK (status IN ('READY', 'RUNNING', 'PAUSED', 'COMPLETED')),
    ADD CONSTRAINT resolver_run_step_bounds
        CHECK (current_step >= 0 AND total_steps >= 0 AND current_step <= total_steps),
    ADD CONSTRAINT resolver_run_plan_sha256_shape CHECK (plan_sha256 ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT resolver_run_auto_interval_bounds
        CHECK (auto_play_interval_ms BETWEEN 500 AND 60000),
    ADD CONSTRAINT resolver_run_auto_shape CHECK (
        (auto_play_enabled AND status = 'RUNNING' AND next_auto_at IS NOT NULL
            AND current_step < total_steps)
        OR (NOT auto_play_enabled AND next_auto_at IS NULL)
    ),
    ADD CONSTRAINT resolver_run_source_pair CHECK (
        (source_public_snapshot_id IS NULL) = (source_final_snapshot_id IS NULL)
    );

UPDATE resolver_runs SET started_at = NULL, completed_at = NULL WHERE status = 'READY';
UPDATE resolver_runs SET started_at = coalesce(started_at, created_at), completed_at = NULL
WHERE status IN ('RUNNING', 'PAUSED');
UPDATE resolver_runs SET current_step = total_steps,
    started_at = coalesce(started_at, created_at), completed_at = coalesce(completed_at, updated_at)
WHERE status = 'COMPLETED';

ALTER TABLE resolver_runs
    ADD CONSTRAINT resolver_run_time_shape CHECK (
        (status = 'READY' AND started_at IS NULL AND completed_at IS NULL)
        OR (status IN ('RUNNING', 'PAUSED') AND started_at IS NOT NULL AND completed_at IS NULL)
        OR (status = 'COMPLETED' AND started_at IS NOT NULL AND completed_at IS NOT NULL
            AND current_step = total_steps)
    );

CREATE UNIQUE INDEX uq_resolver_official_run
    ON resolver_runs (contest_id) WHERE official;

CREATE INDEX idx_resolver_auto_due
    ON resolver_runs (next_auto_at, id)
    WHERE auto_play_enabled AND status = 'RUNNING';

ALTER TABLE resolver_snapshots
    ADD COLUMN state_sha256 char(64),
    ADD CONSTRAINT resolver_snapshot_step_valid CHECK (step_index >= 0),
    ADD CONSTRAINT resolver_snapshot_sha256_shape
        CHECK (state_sha256 IS NULL OR state_sha256 ~ '^[0-9a-f]{64}$');

CREATE UNIQUE INDEX uq_resolver_snapshot_step
    ON resolver_snapshots (run_id, step_index);

ALTER TABLE resolver_current_state
    ADD COLUMN state_sha256 char(64),
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT resolver_current_step_valid CHECK (step_index >= 0),
    ADD CONSTRAINT resolver_current_sha256_shape
        CHECK (state_sha256 IS NULL OR state_sha256 ~ '^[0-9a-f]{64}$');

ALTER TABLE resolver_events
    ADD COLUMN actor_user_id bigint REFERENCES users(id),
    ADD CONSTRAINT resolver_event_sequence_valid CHECK (sequence >= 0);

CREATE UNIQUE INDEX uq_resolver_event_sequence ON resolver_events (run_id, sequence);

CREATE TABLE resolver_pending_submissions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL REFERENCES resolver_runs(id),
    submission_id bigint NOT NULL REFERENCES submissions(id),
    team_id bigint NOT NULL REFERENCES teams(id),
    problem_id bigint NOT NULL REFERENCES problems(id),
    submitted_at timestamptz NOT NULL,
    verdict_at_snapshot varchar(32) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (run_id, submission_id)
);

CREATE INDEX idx_resolver_pending_run_order
    ON resolver_pending_submissions (run_id, team_id, problem_id, submitted_at, submission_id);

CREATE OR REPLACE FUNCTION reject_resolver_history_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'resolver history is immutable';
END;
$$;

CREATE TRIGGER trg_resolver_snapshots_immutable
BEFORE UPDATE OR DELETE ON resolver_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE TRIGGER trg_resolver_events_immutable
BEFORE UPDATE OR DELETE ON resolver_events
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE TRIGGER trg_resolver_pending_immutable
BEFORE UPDATE OR DELETE ON resolver_pending_submissions
FOR EACH ROW EXECUTE FUNCTION reject_resolver_history_mutation();

CREATE OR REPLACE FUNCTION protect_resolver_run_sources()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.contest_id IS DISTINCT FROM OLD.contest_id
       OR NEW.official IS DISTINCT FROM OLD.official
       OR NEW.source_public_snapshot_id IS DISTINCT FROM OLD.source_public_snapshot_id
       OR NEW.source_final_snapshot_id IS DISTINCT FROM OLD.source_final_snapshot_id
       OR NEW.plan_sha256 IS DISTINCT FROM OLD.plan_sha256
       OR NEW.total_steps IS DISTINCT FROM OLD.total_steps
       OR NEW.created_by_user_id IS DISTINCT FROM OLD.created_by_user_id THEN
        RAISE EXCEPTION 'resolver run sources are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_resolver_run_sources_immutable
BEFORE UPDATE ON resolver_runs
FOR EACH ROW EXECUTE FUNCTION protect_resolver_run_sources();

-- ============================================================================
-- Source migration (28/50): 20260719270000_award_integrity.sql
-- ============================================================================

UPDATE award_categories SET code = upper(btrim(code)), name = btrim(name);
UPDATE award_categories SET participation_type = upper(participation_type)
WHERE participation_type IS NOT NULL;

ALTER TABLE award_categories
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT award_category_text_bounds CHECK (
        char_length(code) BETWEEN 1 AND 64 AND code ~ '^[A-Z0-9_]+$'
        AND char_length(name) BETWEEN 1 AND 128
    ),
    ADD CONSTRAINT award_category_order_valid CHECK (display_order BETWEEN 1 AND 1000),
    ADD CONSTRAINT award_category_participation_known CHECK (
        participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')
    );

CREATE UNIQUE INDEX uq_award_category_display_order
    ON award_categories (contest_id, display_order);

DELETE FROM award_rules rule
USING award_rules newer
WHERE rule.category_id = newer.category_id AND rule.id > newer.id;

ALTER TABLE award_rules
    ADD CONSTRAINT award_rule_type_known
        CHECK (rule_type IN ('FIXED_COUNT', 'RATIO', 'RANK_RANGE')),
    ADD CONSTRAINT award_rule_shape CHECK (
        (rule_type = 'FIXED_COUNT' AND fixed_count BETWEEN 1 AND 10000
            AND ratio IS NULL AND rank_from IS NULL AND rank_to IS NULL)
        OR (rule_type = 'RATIO' AND ratio > 0 AND ratio <= 1
            AND fixed_count IS NULL AND rank_from IS NULL AND rank_to IS NULL)
        OR (rule_type = 'RANK_RANGE' AND rank_from >= 1 AND rank_to >= rank_from
            AND fixed_count IS NULL AND ratio IS NULL)
    );

CREATE UNIQUE INDEX uq_award_rule_category ON award_rules (category_id);

CREATE TABLE award_sets (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    contest_id bigint NOT NULL UNIQUE REFERENCES contests(id),
    resolver_run_id bigint NOT NULL UNIQUE REFERENCES resolver_runs(id),
    final_scoreboard_snapshot_id bigint NOT NULL REFERENCES scoreboard_snapshots(id),
    status varchar(16) NOT NULL DEFAULT 'DRAFT',
    generated_by_user_id bigint NOT NULL REFERENCES users(id),
    frozen_by_user_id bigint REFERENCES users(id),
    generated_at timestamptz NOT NULL DEFAULT now(),
    frozen_at timestamptz,
    version integer NOT NULL DEFAULT 0,
    CONSTRAINT award_set_status_known CHECK (status IN ('DRAFT', 'FROZEN')),
    CONSTRAINT award_set_freeze_shape CHECK (
        (status = 'DRAFT' AND frozen_at IS NULL AND frozen_by_user_id IS NULL)
        OR (status = 'FROZEN' AND frozen_at IS NOT NULL AND frozen_by_user_id IS NOT NULL)
    )
);

ALTER TABLE award_recipients
    ADD COLUMN source_scoreboard_snapshot_id bigint REFERENCES scoreboard_snapshots(id),
    ADD COLUMN version integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT award_recipient_participation_known CHECK (
        participation_type IS NULL OR participation_type IN ('OFFICIAL', 'STAR', 'PRACTICE')
    );

CREATE INDEX idx_award_recipients_team_conflicts
    ON award_recipients (contest_id, team_id, category_id);

-- ============================================================================
-- Source migration (29/50): 20260722120000_contest_lifecycle_milestones.sql
-- ============================================================================

CREATE TABLE contest_lifecycle_milestones (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    contest_id bigint NOT NULL REFERENCES contests(id),
    milestone varchar(16) NOT NULL,
    scheduled_at timestamptz NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    previous_status varchar(32) NOT NULL,
    new_status varchar(32) NOT NULL,
    CONSTRAINT ck_contest_lifecycle_milestone CHECK (milestone IN ('STARTED', 'FROZEN', 'ENDED')),
    CONSTRAINT uq_contest_lifecycle_milestone UNIQUE (contest_id, milestone)
);

CREATE INDEX idx_contest_lifecycle_milestones_contest
    ON contest_lifecycle_milestones(contest_id, occurred_at);

CREATE FUNCTION reject_archived_contest_write() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    old_contest_id bigint;
    new_contest_id bigint;
BEGIN
    old_contest_id := CASE WHEN TG_OP IN ('UPDATE', 'DELETE') THEN OLD.contest_id ELSE NULL END;
    new_contest_id := CASE WHEN TG_OP IN ('INSERT', 'UPDATE') THEN NEW.contest_id ELSE NULL END;
    IF EXISTS (
        SELECT 1 FROM contests
        WHERE id IN (old_contest_id, new_contest_id) AND status = 'ARCHIVED'
    ) THEN
        RAISE EXCEPTION 'CONTEST_ARCHIVED_READ_ONLY' USING ERRCODE = 'P0001';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

DO $$
DECLARE
    table_name text;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'announcements', 'award_categories', 'award_certificate_rows',
        'award_host_scripts', 'award_presentation_states', 'award_recipients',
        'balloon_tasks', 'batch_rejudge_tasks', 'clarifications',
        'contest_management_assignments', 'contest_problems', 'contest_teams',
        'presentation_configs', 'print_requests', 'resolver_runs',
        'scoreboard_snapshots', 'screen_groups', 'screen_playlists', 'submissions',
        'award_sets', 'contest_scoreboard_cells', 'contest_scoreboard_rows'
    ] LOOP
        EXECUTE format(
            'CREATE TRIGGER trg_%I_archived_read_only BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_archived_contest_write()',
            table_name, table_name
        );
    END LOOP;
END;
$$;

CREATE FUNCTION reject_archived_contest_write_via_parent() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    old_parent_id bigint;
    new_parent_id bigint;
    archived boolean;
BEGIN
    old_parent_id := CASE WHEN TG_OP IN ('UPDATE', 'DELETE')
        THEN (to_jsonb(OLD) ->> TG_ARGV[0])::bigint ELSE NULL END;
    new_parent_id := CASE WHEN TG_OP IN ('INSERT', 'UPDATE')
        THEN (to_jsonb(NEW) ->> TG_ARGV[0])::bigint ELSE NULL END;
    EXECUTE format(
        'SELECT EXISTS(SELECT 1 FROM %I parent JOIN contests contest ON contest.id=parent.contest_id WHERE parent.id IN ($1,$2) AND contest.status=''ARCHIVED'')',
        TG_ARGV[1]
    ) INTO archived USING old_parent_id, new_parent_id;
    IF archived THEN
        RAISE EXCEPTION 'CONTEST_ARCHIVED_READ_ONLY' USING ERRCODE = 'P0001';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

DO $$
DECLARE
    relation text[];
BEGIN
    FOREACH relation SLICE 1 IN ARRAY ARRAY[
        ARRAY['award_rules','category_id','award_categories'],
        ARRAY['award_host_script_sections','host_script_id','award_host_scripts'],
        ARRAY['batch_rejudge_items','task_id','batch_rejudge_tasks'],
        ARRAY['judgements','submission_id','submissions'],
        ARRAY['submission_outbox','submission_id','submissions'],
        ARRAY['resolver_current_state','run_id','resolver_runs'],
        ARRAY['resolver_events','run_id','resolver_runs'],
        ARRAY['resolver_snapshots','run_id','resolver_runs'],
        ARRAY['resolver_team_states','run_id','resolver_runs'],
        ARRAY['resolver_pending_submissions','run_id','resolver_runs'],
        ARRAY['screen_commands','screen_instance_id','screen_instances'],
        ARRAY['screen_group_members','group_id','screen_groups'],
        ARRAY['screen_playlist_items','playlist_id','screen_playlists']
    ] LOOP
        EXECUTE format(
            'CREATE TRIGGER trg_%I_archived_read_only BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION reject_archived_contest_write_via_parent(%L,%L)',
            relation[1], relation[1], relation[2], relation[3]
        );
    END LOOP;
END;
$$;

CREATE FUNCTION reject_archived_run_write() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    old_judgement uuid;
    new_judgement uuid;
BEGIN
    old_judgement := CASE WHEN TG_OP IN ('UPDATE', 'DELETE') THEN OLD.judgement_id ELSE NULL END;
    new_judgement := CASE WHEN TG_OP IN ('INSERT', 'UPDATE') THEN NEW.judgement_id ELSE NULL END;
    IF EXISTS (
        SELECT 1 FROM judgements judgement
        JOIN submissions submission ON submission.id=judgement.submission_id
        JOIN contests contest ON contest.id=submission.contest_id
        WHERE judgement.id IN (old_judgement, new_judgement) AND contest.status='ARCHIVED'
    ) THEN
        RAISE EXCEPTION 'CONTEST_ARCHIVED_READ_ONLY' USING ERRCODE = 'P0001';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER trg_runs_archived_read_only
BEFORE INSERT OR UPDATE OR DELETE ON runs
FOR EACH ROW EXECUTE FUNCTION reject_archived_run_write();

-- ============================================================================
-- Source migration (30/50): 20260722130000_object_storage_cleanup.sql
-- ============================================================================

CREATE TABLE object_storage_cleanup_tasks (
    id BIGSERIAL PRIMARY KEY,
    bucket VARCHAR(255) NOT NULL,
    object_key TEXT NOT NULL,
    reason VARCHAR(64) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'PENDING',
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_object_storage_cleanup_object UNIQUE (bucket, object_key),
    CONSTRAINT ck_object_storage_cleanup_bucket_nonempty CHECK (length(btrim(bucket)) > 0),
    CONSTRAINT ck_object_storage_cleanup_key_nonempty CHECK (length(btrim(object_key)) > 0),
    CONSTRAINT ck_object_storage_cleanup_reason_nonempty CHECK (length(btrim(reason)) > 0),
    CONSTRAINT ck_object_storage_cleanup_status CHECK (
        status IN ('PENDING', 'PROCESSING', 'FAILED')
    ),
    CONSTRAINT ck_object_storage_cleanup_attempts CHECK (attempts >= 0),
    CONSTRAINT ck_object_storage_cleanup_lease CHECK (
        (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
    )
);

CREATE INDEX idx_object_storage_cleanup_available
    ON object_storage_cleanup_tasks (available_at, id)
    WHERE status IN ('PENDING', 'FAILED');

CREATE INDEX idx_object_storage_cleanup_expired_lease
    ON object_storage_cleanup_tasks (lease_until, id)
    WHERE status = 'PROCESSING';

-- ============================================================================
-- Source migration (31/50): 20260722140000_submission_export_tasks.sql
-- ============================================================================

CREATE TABLE submission_export_tasks (
    id BIGSERIAL PRIMARY KEY,
    contest_id BIGINT NOT NULL REFERENCES contests(id) ON DELETE CASCADE,
    requested_by BIGINT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    kind VARCHAR(32) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'QUEUED',
    output_bucket VARCHAR(255),
    output_object_key TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ck_submission_export_kind CHECK (kind IN ('METADATA_CSV', 'SOURCES_ZIP')),
    CONSTRAINT ck_submission_export_status CHECK (
        status IN ('QUEUED', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'EXPIRED')
    ),
    CONSTRAINT ck_submission_export_attempts CHECK (attempts >= 0),
    CONSTRAINT ck_submission_export_lease CHECK (
        (status = 'PROCESSING' AND lease_owner IS NOT NULL AND lease_until IS NOT NULL)
        OR
        (status <> 'PROCESSING' AND lease_owner IS NULL AND lease_until IS NULL)
    ),
    CONSTRAINT ck_submission_export_output CHECK (
        (status = 'SUCCEEDED' AND output_bucket IS NOT NULL AND output_object_key IS NOT NULL)
        OR status <> 'SUCCEEDED'
    )
);

CREATE INDEX idx_submission_export_available
    ON submission_export_tasks (available_at, id)
    WHERE status IN ('QUEUED', 'FAILED');

CREATE INDEX idx_submission_export_expiry
    ON submission_export_tasks (expires_at, id)
    WHERE status = 'SUCCEEDED';

CREATE INDEX idx_submission_export_contest
    ON submission_export_tasks (contest_id, created_at DESC, id DESC);

-- ============================================================================
-- Source migration (32/50): 20260726000000_allow_archived_scope_cleanup.sql
-- ============================================================================

-- Access-control cleanup must remain possible after a contest is archived.
-- Archived business data is immutable, but removing an obsolete administrator
-- assignment is not a business-data mutation and is required for role/scope
-- maintenance.
DROP TRIGGER IF EXISTS trg_contest_management_assignments_archived_read_only
    ON contest_management_assignments;

CREATE TRIGGER trg_contest_management_assignments_archived_read_only
BEFORE INSERT OR UPDATE ON contest_management_assignments
FOR EACH ROW EXECUTE FUNCTION reject_archived_contest_write();

-- ============================================================================
-- Source migration (33/50): 20260729000000_object_storage_integrity.sql
-- ============================================================================

CREATE TABLE object_storage_integrity_findings (
    id BIGSERIAL PRIMARY KEY,
    bucket VARCHAR(255) NOT NULL,
    object_key TEXT NOT NULL,
    first_detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    CONSTRAINT uq_object_storage_integrity_object UNIQUE (bucket, object_key),
    CONSTRAINT ck_object_storage_integrity_bucket_nonempty CHECK (length(btrim(bucket)) > 0),
    CONSTRAINT ck_object_storage_integrity_key_nonempty CHECK (length(btrim(object_key)) > 0),
    CONSTRAINT ck_object_storage_integrity_timestamps CHECK (
        last_detected_at >= first_detected_at
        AND (resolved_at IS NULL OR resolved_at >= first_detected_at)
    )
);

CREATE INDEX idx_object_storage_integrity_unresolved
    ON object_storage_integrity_findings (last_detected_at, id)
    WHERE resolved_at IS NULL;

-- ============================================================================
-- Source migration (34/50): 20260729010000_submission_similarity.sql
-- ============================================================================

ALTER TABLE submissions
    ADD COLUMN source_fingerprint varchar(64),
    ADD COLUMN source_simhash bigint,
    ADD COLUMN source_token_count integer;

ALTER TABLE submissions
    ADD CONSTRAINT submissions_source_fingerprint_format
    CHECK (source_fingerprint IS NULL OR source_fingerprint ~ '^[0-9a-f]{64}$');

ALTER TABLE submissions
    ADD CONSTRAINT submissions_similarity_shape
    CHECK (
        (source_simhash IS NULL AND source_token_count IS NULL)
        OR (source_simhash IS NOT NULL AND source_token_count > 0)
    );

CREATE INDEX idx_submissions_similarity
    ON submissions (contest_id, problem_id, language, source_fingerprint)
    WHERE source_fingerprint IS NOT NULL;

CREATE INDEX idx_submissions_simhash
    ON submissions (contest_id, problem_id, language, source_simhash)
    WHERE source_simhash IS NOT NULL;

COMMENT ON COLUMN submissions.source_fingerprint IS
    'SHA-256 of source with comments and formatting whitespace removed; used for exact normalized duplicate detection';

COMMENT ON COLUMN submissions.source_simhash IS
    '64-bit SimHash over normalized five-token shingles for approximate similarity screening';

-- ============================================================================
-- Source migration (35/50): 20260729020000_presentation_templates.sql
-- ============================================================================

ALTER TABLE presentation_configs
    ADD COLUMN template varchar(32) NOT NULL DEFAULT 'DEFAULT';

ALTER TABLE presentation_configs
    ADD CONSTRAINT presentation_configs_template_known
    CHECK (template IN ('DEFAULT', 'CINEMATIC', 'MINIMAL', 'SPLIT'));

-- ============================================================================
-- Source migration (36/50): 20260729030000_oi_ioi_scoring.sql
-- ============================================================================

ALTER TABLE contests
    ADD COLUMN scoring_mode varchar(16) NOT NULL DEFAULT 'ICPC',
    ADD COLUMN score_aggregation varchar(16) NOT NULL DEFAULT 'BEST',
    ADD COLUMN feedback_policy varchar(16) NOT NULL DEFAULT 'FULL',
    ADD CONSTRAINT contests_scoring_mode_known
        CHECK (scoring_mode IN ('ICPC', 'OI', 'IOI')),
    ADD CONSTRAINT contests_score_aggregation_known
        CHECK (score_aggregation IN ('BEST', 'LAST')),
    ADD CONSTRAINT contests_feedback_policy_known
        CHECK (feedback_policy IN ('FULL', 'SCORE_ONLY', 'NONE'));

ALTER TABLE contest_problems
    ADD COLUMN max_score_milli integer NOT NULL DEFAULT 100000,
    ADD CONSTRAINT contest_problems_max_score_valid
        CHECK (max_score_milli BETWEEN 1 AND 100000000);

CREATE TABLE contest_problem_subtasks (
    id bigserial PRIMARY KEY,
    contest_id bigint NOT NULL,
    problem_id bigint NOT NULL,
    subtask_key varchar(32) NOT NULL,
    name varchar(120) NOT NULL,
    display_order integer NOT NULL,
    score_milli integer NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contest_problem_subtasks_assignment_fk
        FOREIGN KEY (contest_id, problem_id)
        REFERENCES contest_problems(contest_id, problem_id) ON DELETE CASCADE,
    CONSTRAINT contest_problem_subtasks_key_shape
        CHECK (subtask_key ~ '^[A-Z0-9_]{1,32}$'),
    CONSTRAINT contest_problem_subtasks_name_bounds
        CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    CONSTRAINT contest_problem_subtasks_order_valid
        CHECK (display_order BETWEEN 1 AND 1000),
    CONSTRAINT contest_problem_subtasks_score_valid
        CHECK (score_milli BETWEEN 1 AND 100000000),
    CONSTRAINT contest_problem_subtasks_key_unique
        UNIQUE (contest_id, problem_id, subtask_key),
    CONSTRAINT contest_problem_subtasks_order_unique
        UNIQUE (contest_id, problem_id, display_order)
);

CREATE TABLE contest_problem_subtask_tests (
    subtask_id bigint NOT NULL REFERENCES contest_problem_subtasks(id) ON DELETE CASCADE,
    test_index integer NOT NULL CHECK (test_index BETWEEN 1 AND 10000),
    PRIMARY KEY (subtask_id, test_index)
);

CREATE INDEX idx_contest_problem_subtasks_assignment
    ON contest_problem_subtasks(contest_id, problem_id, display_order);

ALTER TABLE judgements
    ADD COLUMN score_milli integer NOT NULL DEFAULT 0,
    ADD CONSTRAINT judgements_score_valid
        CHECK (score_milli BETWEEN 0 AND 100000000);

CREATE TABLE judgement_subtask_scores (
    judgement_id uuid NOT NULL REFERENCES judgements(id) ON DELETE CASCADE,
    subtask_id bigint NOT NULL REFERENCES contest_problem_subtasks(id),
    score_milli integer NOT NULL,
    passed_tests integer NOT NULL,
    total_tests integer NOT NULL,
    PRIMARY KEY (judgement_id, subtask_id),
    CONSTRAINT judgement_subtask_score_valid CHECK (score_milli >= 0),
    CONSTRAINT judgement_subtask_counts_valid
        CHECK (total_tests > 0 AND passed_tests BETWEEN 0 AND total_tests)
);

ALTER TABLE contest_scoreboard_cells
    ADD COLUMN score_milli integer NOT NULL DEFAULT 0,
    ADD COLUMN effective_submission_id bigint REFERENCES submissions(id),
    ADD CONSTRAINT scoreboard_cell_score_valid CHECK (score_milli >= 0);

ALTER TABLE contest_scoreboard_rows
    ADD COLUMN total_score_milli bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT scoreboard_row_score_valid CHECK (total_score_milli >= 0);

CREATE INDEX idx_judgements_submission_score
    ON judgements(submission_id, score_milli DESC)
    WHERE active_marker IS TRUE AND completed_at IS NOT NULL;

-- ============================================================================
-- Source migration (37/50): 20260729040000_judge_modes.sql
-- ============================================================================

ALTER TABLE problems
    ADD COLUMN judge_mode varchar(20) NOT NULL DEFAULT 'STANDARD',
    ADD COLUMN interactor_object_key varchar(512),
    ADD COLUMN interactor_sha256 varchar(64),
    ADD CONSTRAINT problems_judge_mode_known
        CHECK (judge_mode IN ('STANDARD', 'INTERACTIVE', 'OUTPUT_ONLY')),
    ADD CONSTRAINT problems_interactor_pair
        CHECK ((judge_mode = 'INTERACTIVE' AND interactor_object_key IS NOT NULL
                AND interactor_sha256 ~ '^[0-9a-f]{64}$')
            OR (judge_mode <> 'INTERACTIVE' AND interactor_object_key IS NULL
                AND interactor_sha256 IS NULL)),
    ADD CONSTRAINT problems_mode_language_shape CHECK (
        (judge_mode = 'OUTPUT_ONLY' AND languages::jsonb = '["output"]'::jsonb)
        OR (judge_mode <> 'OUTPUT_ONLY' AND NOT languages::jsonb ? 'output')
    );

ALTER TABLE submissions
    ADD CONSTRAINT submissions_output_language_allowed
        CHECK (language <> 'output' OR source_object_key ~ '[.]zip$');

-- ============================================================================
-- Source migration (38/50): 20260729050000_problem_bank_training.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.problem_bank_entries (
    problem_id bigint PRIMARY KEY REFERENCES public.problems(id) ON DELETE CASCADE,
    visibility character varying(16) NOT NULL DEFAULT 'PRIVATE',
    difficulty smallint,
    tags text NOT NULL DEFAULT '[]',
    published_at timestamp with time zone,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT problem_bank_visibility_known CHECK (visibility IN ('PRIVATE', 'PUBLIC')),
    CONSTRAINT problem_bank_difficulty_valid CHECK (difficulty IS NULL OR difficulty BETWEEN 0 AND 10),
    CONSTRAINT problem_bank_tags_json CHECK (jsonb_typeof(tags::jsonb) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_problem_bank_public
    ON public.problem_bank_entries (published_at DESC, problem_id DESC)
    WHERE visibility = 'PUBLIC';

CREATE TABLE IF NOT EXISTS public.training_sets (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    slug character varying(96) NOT NULL UNIQUE,
    title character varying(255) NOT NULL,
    description text NOT NULL DEFAULT '',
    visibility character varying(16) NOT NULL DEFAULT 'DRAFT',
    created_by_user_id bigint REFERENCES public.users(id),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT training_set_visibility_known CHECK (visibility IN ('DRAFT', 'PUBLIC', 'ARCHIVED'))
);

CREATE TABLE IF NOT EXISTS public.training_set_items (
    set_id bigint NOT NULL REFERENCES public.training_sets(id) ON DELETE CASCADE,
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE RESTRICT,
    position integer NOT NULL,
    required boolean NOT NULL DEFAULT false,
    PRIMARY KEY (set_id, problem_id),
    CONSTRAINT training_set_item_position_valid CHECK (position > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_training_set_items_position
    ON public.training_set_items (set_id, position);

CREATE TABLE IF NOT EXISTS public.training_enrollments (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    set_id bigint NOT NULL REFERENCES public.training_sets(id) ON DELETE CASCADE,
    team_id bigint NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,
    status character varying(16) NOT NULL DEFAULT 'ACTIVE',
    started_at timestamp with time zone NOT NULL DEFAULT now(),
    completed_at timestamp with time zone,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    UNIQUE (set_id, team_id),
    CONSTRAINT training_enrollment_status_known CHECK (status IN ('ACTIVE', 'COMPLETED', 'ABANDONED'))
);

CREATE TABLE IF NOT EXISTS public.training_progress (
    enrollment_id bigint NOT NULL REFERENCES public.training_enrollments(id) ON DELETE CASCADE,
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE RESTRICT,
    status character varying(16) NOT NULL DEFAULT 'TODO',
    attempts integer NOT NULL DEFAULT 0,
    best_score integer NOT NULL DEFAULT 0,
    solved_at timestamp with time zone,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (enrollment_id, problem_id),
    CONSTRAINT training_progress_status_known CHECK (status IN ('TODO', 'IN_PROGRESS', 'SOLVED')),
    CONSTRAINT training_progress_values_valid CHECK (attempts >= 0 AND best_score BETWEEN 0 AND 100)
);

CREATE INDEX IF NOT EXISTS idx_training_enrollments_team ON public.training_enrollments(team_id, updated_at DESC);

-- ============================================================================
-- Source migration (39/50): 20260729060000_custom_presentation_templates.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.presentation_templates (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    name character varying(120) NOT NULL,
    description character varying(500) NOT NULL DEFAULT '',
    background_color character varying(7) NOT NULL DEFAULT '#07111f',
    foreground_color character varying(7) NOT NULL DEFAULT '#ffffff',
    accent_color character varying(7) NOT NULL DEFAULT '#22c55e',
    font_family character varying(120) NOT NULL DEFAULT 'Inter',
    density character varying(16) NOT NULL DEFAULT 'COMFORTABLE',
    show_clock boolean NOT NULL DEFAULT true,
    show_logo boolean NOT NULL DEFAULT false,
    logo_object_key character varying(512),
    created_by_user_id bigint REFERENCES public.users(id),
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT presentation_template_colors_valid CHECK (background_color ~ '^#[0-9A-Fa-f]{6}$' AND foreground_color ~ '^#[0-9A-Fa-f]{6}$' AND accent_color ~ '^#[0-9A-Fa-f]{6}$'),
    CONSTRAINT presentation_template_density_known CHECK (density IN ('COMPACT', 'COMFORTABLE', 'SPACIOUS'))
);

ALTER TABLE public.presentation_configs
    ADD COLUMN IF NOT EXISTS custom_template_id bigint REFERENCES public.presentation_templates(id) ON DELETE SET NULL;

ALTER TABLE public.presentation_configs DROP CONSTRAINT IF EXISTS presentation_configs_template_known;
ALTER TABLE public.presentation_configs
    ADD CONSTRAINT presentation_configs_template_known
    CHECK ((template IN ('DEFAULT', 'CINEMATIC', 'MINIMAL', 'SPLIT') AND custom_template_id IS NULL) OR (template = 'CUSTOM' AND custom_template_id IS NOT NULL));

-- ============================================================================
-- Source migration (40/50): 20260729070000_balloon_dispatch_policy.sql
-- ============================================================================

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

-- ============================================================================
-- Source migration (41/50): 20260730010000_practice_submissions.sql
-- ============================================================================

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

-- ============================================================================
-- Source migration (42/50): 20260730020000_practice_library.sql
-- ============================================================================

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

-- ============================================================================
-- Source migration (43/50): 20260730030000_virtual_practice.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.practice_virtual_sessions (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    user_id bigint NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    title character varying(255) NOT NULL,
    start_at timestamp with time zone NOT NULL,
    end_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    CONSTRAINT practice_virtual_time_valid CHECK (end_at > start_at AND end_at <= start_at + interval '7 days')
);

CREATE TABLE IF NOT EXISTS public.practice_virtual_items (
    session_id bigint NOT NULL REFERENCES public.practice_virtual_sessions(id) ON DELETE CASCADE,
    problem_id bigint NOT NULL REFERENCES public.problems(id) ON DELETE RESTRICT,
    position integer NOT NULL,
    PRIMARY KEY (session_id, problem_id),
    UNIQUE (session_id, position),
    CONSTRAINT practice_virtual_position_valid CHECK (position > 0)
);

ALTER TABLE public.submissions
    ADD COLUMN IF NOT EXISTS virtual_session_id bigint REFERENCES public.practice_virtual_sessions(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_submissions_virtual_session ON public.submissions(virtual_session_id,submitted_at,id) WHERE virtual_session_id IS NOT NULL;

-- ============================================================================
-- Source migration (44/50): 20260730040000_practice_limits.sql
-- ============================================================================

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

-- ============================================================================
-- Source migration (45/50): 20260730050000_practice_source_retention.sql
-- ============================================================================

ALTER TABLE public.submissions
    ADD COLUMN IF NOT EXISTS source_deleted_at timestamp with time zone;

CREATE INDEX IF NOT EXISTS submissions_practice_retention_idx
    ON public.submissions (submitted_at)
    WHERE submission_scope = 'PRACTICE' AND source_deleted_at IS NULL;

-- ============================================================================
-- Source migration (46/50): 20260730060000_virtual_practice_archive.sql
-- ============================================================================

ALTER TABLE public.practice_virtual_sessions
    ADD COLUMN IF NOT EXISTS archived_at timestamp with time zone;

-- ============================================================================
-- Source migration (47/50): 20260801000000_scoreboard_scoring_revision.sql
-- ============================================================================

-- Scoring and feedback policy are part of the scoreboard projection contract.
-- Keep the same revision keying used by roster/problem/status changes so a
-- cached public or administrative board cannot outlive a policy update.
CREATE OR REPLACE FUNCTION preserve_or_bump_contest_scoreboard_revision()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status
       OR OLD.start_at IS DISTINCT FROM NEW.start_at
       OR OLD.freeze_at IS DISTINCT FROM NEW.freeze_at
       OR OLD.end_at IS DISTINCT FROM NEW.end_at
       OR OLD.scoring_mode IS DISTINCT FROM NEW.scoring_mode
       OR OLD.score_aggregation IS DISTINCT FROM NEW.score_aggregation
       OR OLD.feedback_policy IS DISTINCT FROM NEW.feedback_policy
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at THEN
        NEW.scoreboard_revision := OLD.scoreboard_revision + 1;
    ELSE
        NEW.scoreboard_revision := greatest(NEW.scoreboard_revision, OLD.scoreboard_revision);
    END IF;
    RETURN NEW;
END;
$$;

-- ============================================================================
-- Source migration (48/50): 20260801010000_realtime_outbox_lease.sql
-- ============================================================================

ALTER TABLE realtime_outbox
    ADD COLUMN lease_owner uuid,
    ADD CONSTRAINT realtime_outbox_lease_shape CHECK (
        (status = 'PUBLISHING' AND lease_owner IS NOT NULL)
        OR
        (status <> 'PUBLISHING' AND lease_owner IS NULL)
    );

CREATE INDEX idx_realtime_outbox_expired_lease
    ON realtime_outbox (available_at, id)
    WHERE status = 'PUBLISHING';

-- ============================================================================
-- Source migration (49/50): 20260801020000_practice_progress_index.sql
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_practice_progress_user_updated
    ON public.practice_problem_progress (user_id, updated_at DESC, problem_id);

-- ============================================================================
-- Source migration (50/50): 20260809000000_competition_mode.sql
-- ============================================================================

CREATE TABLE competition_workstations (
    id bigserial PRIMARY KEY,
    ip_address varchar(45) NOT NULL UNIQUE,
    seat_no varchar(64) NOT NULL UNIQUE,
    label varchar(128),
    enabled boolean NOT NULL DEFAULT true,
    last_seen_at timestamptz,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT competition_workstations_ip_not_blank CHECK (btrim(ip_address) <> ''),
    CONSTRAINT competition_workstations_seat_not_blank CHECK (btrim(seat_no) <> ''),
    CONSTRAINT competition_workstations_label_not_blank
        CHECK (label IS NULL OR btrim(label) <> '')
);

CREATE TABLE contest_workstation_bindings (
    id bigserial PRIMARY KEY,
    contest_id bigint NOT NULL REFERENCES contests(id) ON DELETE CASCADE,
    workstation_id bigint NOT NULL REFERENCES competition_workstations(id) ON DELETE CASCADE,
    team_id bigint NOT NULL,
    pairing_code_hash character(64) NOT NULL,
    bound_by_user_id bigint NOT NULL REFERENCES users(id),
    bound_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contest_workstation_bindings_roster_fkey
        FOREIGN KEY (contest_id, team_id)
        REFERENCES contest_teams(contest_id, team_id)
        ON DELETE CASCADE,
    CONSTRAINT contest_workstation_bindings_pairing_hash
        CHECK (pairing_code_hash ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX idx_contest_workstation_active_terminal
    ON contest_workstation_bindings(contest_id, workstation_id)
    WHERE revoked_at IS NULL;

CREATE UNIQUE INDEX idx_contest_workstation_active_team
    ON contest_workstation_bindings(contest_id, team_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_contest_workstation_active_lookup
    ON contest_workstation_bindings(workstation_id, contest_id)
    WHERE revoked_at IS NULL;

ALTER TABLE auth_sessions
    ADD COLUMN workstation_binding_id bigint
        REFERENCES contest_workstation_bindings(id) ON DELETE CASCADE,
    ADD COLUMN bound_ip varchar(45),
    ADD CONSTRAINT auth_sessions_workstation_shape CHECK (
        (workstation_binding_id IS NULL AND bound_ip IS NULL)
        OR (workstation_binding_id IS NOT NULL AND bound_ip IS NOT NULL)
    );

CREATE INDEX idx_auth_sessions_workstation_binding
    ON auth_sessions(workstation_binding_id)
    WHERE workstation_binding_id IS NOT NULL;



-- ============================================================================
-- Post-amalgamation: domain status CHECK constraints
-- (contests.status / submissions.status were the two domain status columns
--  left without CHECK when every other status column gained one in the
--  consolidated source migrations; alpha-stage edit per migrations/README.md)
-- ============================================================================
ALTER TABLE contests
    ADD CONSTRAINT contest_status_known
        CHECK (status IN ('DRAFT', 'FROZEN_CONFIG', 'RUNNING', 'PAUSED', 'ENDED', 'ARCHIVED'));

-- Technical debt: submissions.status overloads two semantics — the judging
-- pipeline state (PENDING/JUDGING/CANCELLED) and the final verdict, because
-- result_processor writes `SET status = $2` bound to verdict.as_str().
-- Long term this should be split into dedicated status/verdict columns; the
-- CHECK below must cover both until then (see SubmissionStatus as_str).
ALTER TABLE submissions
    ADD CONSTRAINT submission_status_known
        CHECK (status IN (
            'PENDING', 'JUDGING', 'CANCELLED',
            'ACCEPTED', 'WRONG_ANSWER', 'COMPILE_ERROR', 'RUNTIME_ERROR',
            'TIME_LIMIT_EXCEEDED', 'MEMORY_LIMIT_EXCEEDED',
            'OUTPUT_LIMIT_EXCEEDED', 'SYSTEM_ERROR'
        ));

-- ============================================================================
-- Post-amalgamation: split submissions.status (lifecycle) from verdict
-- submissions.status historically overloaded the judging lifecycle
-- (PENDING/JUDGING) with the final verdict string written by
-- result_processor. status now only carries the lifecycle
-- (PENDING -> JUDGING -> COMPLETED) and the new verdict column carries the
-- JudgeVerdict (null until the submission completes). CANCELLED remains a
-- verdict, not a lifecycle state.
-- ============================================================================
ALTER TABLE submissions DROP CONSTRAINT submission_status_known;

ALTER TABLE submissions ADD COLUMN verdict varchar(32) NULL;

ALTER TABLE submissions
    ADD CONSTRAINT submission_verdict_known CHECK (
        verdict IN ('ACCEPTED','WRONG_ANSWER','COMPILE_ERROR','RUNTIME_ERROR',
                    'TIME_LIMIT_EXCEEDED','MEMORY_LIMIT_EXCEEDED',
                    'OUTPUT_LIMIT_EXCEEDED','SYSTEM_ERROR','CANCELLED'));

ALTER TABLE submissions
    ADD CONSTRAINT submission_status_verdict_consistent CHECK (
        (status = 'COMPLETED' AND verdict IS NOT NULL)
        OR (status IN ('PENDING','JUDGING') AND verdict IS NULL));

-- Backfill: verdict literals historically lived in the status column.
UPDATE submissions SET verdict = status
  WHERE status IN ('ACCEPTED','WRONG_ANSWER','COMPILE_ERROR','RUNTIME_ERROR',
                   'TIME_LIMIT_EXCEEDED','MEMORY_LIMIT_EXCEEDED',
                   'OUTPUT_LIMIT_EXCEEDED','SYSTEM_ERROR','CANCELLED');
UPDATE submissions SET status = 'COMPLETED' WHERE verdict IS NOT NULL;

ALTER TABLE submissions
    ADD CONSTRAINT submission_status_known
        CHECK (status IN ('PENDING','JUDGING','COMPLETED'));

CREATE INDEX idx_submissions_verdict ON submissions (verdict) WHERE verdict IS NOT NULL;
