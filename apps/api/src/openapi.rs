use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    features::{
        announcements, audit_logs, auth, awards, balloons, clarifications,
        contest_management_scopes, contest_problems, contests, presentation, printing, problems,
        realtime, resolver, scoreboard, scoring, staff_accounts, submissions, teams, training,
        virtual_practice,
    },
    health, metrics,
};

const OPENAPI_JSON_PATH: &str = "/api/openapi.json";
const SWAGGER_UI_PATH: &str = "/api/docs";

#[derive(OpenApi)]
#[openapi(
    info(
        title = "ProjectBalloon API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Runtime OpenAPI contract generated from the Rust implementation. The legacy docs/api/openapi.yaml file remains a migration compatibility baseline."
    ),
    paths(
        health::liveness,
        health::readiness,
        metrics::prometheus,
        auth::csrf::csrf,
        auth::handlers::login,
        auth::handlers::register,
        auth::handlers::logout,
        auth::handlers::current_user,
        auth::handlers::update_profile,
        auth::handlers::change_password,
        contests::handlers::list,
        contests::handlers::get,
        contests::handlers::create,
        contests::handlers::update,
        contests::handlers::delete,
        contests::handlers::transition,
        contests::handlers::extend,
        contests::handlers::clone_contest,
        teams::handlers::create,
        teams::handlers::list,
        teams::handlers::get,
        teams::handlers::update,
        teams::handlers::delete,
        teams::handlers::batch_import,
        teams::handlers::list_members,
        teams::handlers::add_member,
        teams::handlers::update_member,
        teams::handlers::remove_member,
        teams::handlers::reset_password,
        teams::handlers::list_contest_teams,
        teams::handlers::assign_to_contest,
        teams::handlers::remove_from_contest,
        problems::handlers::list,
        problems::handlers::get,
        problems::handlers::create,
        problems::handlers::update,
        problems::handlers::delete,
        problems::handlers::list_statements,
        problems::handlers::delete_statement,
        problems::handlers::upsert_statement,
        problems::handlers::list_attachments,
        problems::handlers::upload_attachment,
        problems::handlers::download_attachment,
        problems::handlers::delete_attachment,
        problems::handlers::upload_testdata,
        problems::handlers::upload_interactor,
        problems::handlers::download_testdata,
        problems::handlers::list_testdata_versions,
        problems::handlers::download_testdata_version,
        problems::handlers::activate_testdata_version,
        contest_problems::handlers::list,
        contest_problems::handlers::assign,
        contest_problems::handlers::update,
        contest_problems::handlers::remove,
        contest_problems::handlers::reorder,
        submissions::handlers::list_own,
        submissions::handlers::list_admin,
        submissions::handlers::list_similarity,
        submissions::handlers::list_similarity_pairs,
        submissions::handlers::backfill_similarity,
        submissions::handlers::detail_own,
        submissions::handlers::detail_admin,
        submissions::handlers::submit,
        submissions::handlers::submit_practice,
        submissions::handlers::list_practice,
        submissions::handlers::practice_progress,
        submissions::handlers::practice_detail,
        submissions::handlers::rejudge,
        training::list_bank,
        training::get_bank,
        training::get_publication,
        training::update_publication,
        training::create_set,
        training::update_set,
        training::list_sets,
        training::get_set,
        training::enroll,
        training::progress,
        training::list_favorites,
        training::set_favorite,
        training::get_editorial,
        training::get_admin_editorial,
        training::upsert_editorial,
        training::get_practice_settings,
        training::update_practice_settings,
        virtual_practice::create,
        virtual_practice::list,
        virtual_practice::get,
        virtual_practice::archive,
        submissions::handlers::export_metadata_csv,
        submissions::handlers::export_sources_zip,
        submissions::handlers::preview_batch_rejudge,
        submissions::handlers::create_batch_rejudge,
        submissions::handlers::list_batch_rejudge,
        submissions::handlers::get_batch_rejudge,
        submissions::handlers::pause_batch_rejudge,
        submissions::handlers::resume_batch_rejudge,
        scoreboard::handlers::public,
        scoreboard::handlers::public_csv,
        scoreboard::handlers::admin,
        scoreboard::handlers::admin_csv,
        scoreboard::handlers::create_snapshot,
        scoreboard::handlers::latest_snapshot,
        scoring::get_policy,
        scoring::update_policy,
        scoring::get_subtasks,
        scoring::replace_subtasks,
        printing::create,
        printing::list_mine,
        printing::list_all,
        printing::download_pdf,
        printing::retry,
        printing::cancel,
        printing::reject,
        balloons::list,
        balloons::stats,
        balloons::claim,
        balloons::deliver,
        balloons::cancel,
        balloons::reopen,
        balloons::note,
        clarifications::ask,
        clarifications::list_mine,
        clarifications::list_all,
        clarifications::get,
        clarifications::reply,
        clarifications::close,
        clarifications::convert,
        resolver::list,
        resolver::create,
        resolver::sources,
        resolver::get,
        resolver::events,
        resolver::public_state,
        resolver::start,
        resolver::next,
        resolver::previous,
        resolver::pause,
        resolver::resume,
        resolver::complete,
        resolver::auto_play,
        awards::list_categories,
        awards::create_category,
        awards::update_category,
        awards::delete_category,
        awards::completed_resolver_runs,
        awards::generate,
        awards::get,
        awards::manual_add,
        awards::candidates,
        awards::manual_remove,
        awards::freeze,
        awards::unfreeze,
        awards::csv,
        awards::public_presentation,
        awards::update_presentation,
        awards::get_host_script,
        awards::save_host_script,
        awards::certificate_export,
        presentation::get_config,
        presentation::update_screen,
        presentation::update_live,
        presentation::register,
        presentation::heartbeat,
        presentation::list_instances,
        presentation::command,
        presentation::revoke,
        presentation::published,
        presentation::metrics,
        presentation::list_tokens,
        presentation::create_token,
        presentation::revoke_token,
        presentation::list_playlists,
        presentation::create_playlist,
        presentation::update_playlist,
        presentation::delete_playlist,
        presentation::list_groups,
        presentation::create_group,
        presentation::update_group,
        presentation::control_group,
        presentation::delete_group,
        announcements::list,
        announcements::create,
        announcements::get,
        announcements::update,
        announcements::pin,
        announcements::schedule,
        announcements::cancel,
        announcements::withdraw,
        submissions::handlers::judge_queue_status,
        submissions::handlers::create_export_task,
        submissions::handlers::get_export_task,
        submissions::handlers::download_export_task,
        staff_accounts::handlers::list,
        staff_accounts::handlers::create,
        staff_accounts::handlers::update,
        staff_accounts::handlers::reset_password,
        contest_management_scopes::handlers::list,
        contest_management_scopes::handlers::replace,
        audit_logs::handlers::list,
        realtime::handlers::subscribe_public,
        realtime::handlers::subscribe_staff,
        realtime::handlers::subscribe_team,
    ),
    tags(
        (name = "health", description = "Process and dependency health probes"),
        (name = "auth", description = "Session and CSRF authentication"),
        (name = "contests", description = "Contest management and lifecycle"),
        (name = "teams", description = "Teams, members, accounts, and contest roster"),
        (name = "problems", description = "Problem catalog, statements, attachments, and test data"),
        (name = "contest-problems", description = "Contest problem assignment and ordering"),
        (name = "submissions", description = "Submission, judging, rejudge, and export operations"),
        (name = "training", description = "Public problem bank and training sets"),
        (name = "practice", description = "Daily practice, editorials, favorites, and virtual sessions"),
        (name = "scoreboard", description = "Public, administrative, CSV, and snapshot scoreboards"),
        (name = "scoring", description = "OI/IOI scoring policies and subtask configuration"),
        (name = "printing", description = "Print requests and operator workflow"),
        (name = "balloons", description = "Balloon delivery workflow"),
        (name = "clarifications", description = "Team questions and staff replies"),
        (name = "resolver", description = "Resolver runs, events, public state, and playback control"),
        (name = "awards", description = "Award rules, recipients, presentation, scripts, and certificates"),
        (name = "presentation", description = "Presentation configuration and public live views"),
        (name = "screens", description = "Screen registration, playback, playlists, and groups"),
        (name = "live", description = "Live presentation tokens and metrics"),
        (name = "announcements", description = "Contest announcement publication workflow"),
        (name = "judge-queue", description = "Judge queue operations and observability"),
        (name = "staff-accounts", description = "Staff account administration"),
        (name = "admin-scopes", description = "Contest manager scope management"),
        (name = "audit-logs", description = "Administrative audit log queries"),
        (name = "realtime", description = "Server-sent contest event streams"),
        (name = "observability", description = "Prometheus operational metrics")
    ),
    modifiers(&SecuritySchemes)
)]
pub struct ApiDoc;

struct SecuritySchemes;

impl Modify for SecuritySchemes {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("PB_SESSION"))),
        );
        components.add_security_scheme(
            "csrf_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("XSRF-TOKEN"))),
        );
        components.add_security_scheme(
            "csrf_header",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-XSRF-TOKEN"))),
        );
        components.add_security_scheme(
            "broadcast_token_header",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Broadcast-Token"))),
        );
    }
}

#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

#[must_use]
pub fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new(SWAGGER_UI_PATH).url(OPENAPI_JSON_PATH, document())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::{Router, body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{document, swagger_ui};

    const EXPECTED_OPERATIONS: &[(&str, &str)] = &[
        ("/livez", "get"),
        ("/api/health", "get"),
        ("/metrics", "get"),
        ("/api/auth/csrf", "get"),
        ("/api/auth/login", "post"),
        ("/api/auth/logout", "post"),
        ("/api/auth/me", "get"),
        ("/api/auth/password", "post"),
        ("/api/contests", "get"),
        ("/api/contests", "post"),
        ("/api/contests/{contest_id}", "get"),
        ("/api/contests/{contest_id}", "patch"),
        ("/api/contests/{contest_id}", "delete"),
        ("/api/contests/{contest_id}/transitions", "post"),
        ("/api/contests/{contest_id}/extensions", "post"),
        ("/api/contests/{source_contest_id}/clones", "post"),
        ("/api/teams", "get"),
        ("/api/teams", "post"),
        ("/api/teams/{team_id}", "get"),
        ("/api/teams/{team_id}", "patch"),
        ("/api/teams/{team_id}", "delete"),
        ("/api/teams/batch", "post"),
        ("/api/teams/{team_id}/members", "get"),
        ("/api/teams/{team_id}/members", "post"),
        ("/api/teams/{team_id}/members/{member_id}", "patch"),
        ("/api/teams/{team_id}/members/{member_id}", "delete"),
        ("/api/teams/{team_id}/account/reset-password", "post"),
        ("/api/contests/{contest_id}/teams", "get"),
        ("/api/contests/{contest_id}/teams", "post"),
        ("/api/contests/{contest_id}/teams/{team_id}", "delete"),
        ("/api/problems", "get"),
        ("/api/problems", "post"),
        ("/api/problems/{problem_id}", "get"),
        ("/api/problems/{problem_id}", "patch"),
        ("/api/problems/{problem_id}", "delete"),
        ("/api/problems/{problem_id}/statements", "get"),
        ("/api/problems/{problem_id}/statements/{lang_code}", "put"),
        ("/api/problems/{problem_id}/statements/{lang_code}", "delete"),
        ("/api/problems/{problem_id}/attachments", "get"),
        ("/api/problems/{problem_id}/attachments", "post"),
        ("/api/problems/{problem_id}/attachments/{attachment_id}", "get"),
        ("/api/problems/{problem_id}/attachments/{attachment_id}", "delete"),
        ("/api/problems/{problem_id}/testdata", "get"),
        ("/api/problems/{problem_id}/testdata", "post"),
        ("/api/problems/{problem_id}/testdata/versions", "get"),
        ("/api/problems/{problem_id}/testdata/versions/{version}", "get"),
        ("/api/problems/{problem_id}/testdata/versions/{version}/activate", "post"),
        ("/api/contests/{contest_id}/problems", "get"),
        ("/api/contests/{contest_id}/problems", "post"),
        ("/api/contests/{contest_id}/problems/{problem_id}", "patch"),
        ("/api/contests/{contest_id}/problems/{problem_id}", "delete"),
        ("/api/contests/{contest_id}/problems/reorder", "put"),
        ("/api/contests/{contest_id}/submissions", "get"),
        ("/api/contests/{contest_id}/submissions", "post"),
        ("/api/contests/{contest_id}/submissions/{submission_id}", "get"),
        ("/api/admin/contests/{contest_id}/submissions", "get"),
        ("/api/admin/contests/{contest_id}/submissions/{submission_id}", "get"),
        ("/api/admin/contests/{contest_id}/submissions/{submission_id}/rejudge", "post"),
        ("/api/admin/contests/{contest_id}/exports/submissions.csv", "get"),
        ("/api/admin/contests/{contest_id}/exports/submission-sources.zip", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/preview", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/pause", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/resume", "post"),
        ("/api/admin/contests/{contest_id}/scoreboard", "get"),
        ("/api/admin/contests/{contest_id}/scoreboard.csv", "get"),
        ("/api/admin/contests/{contest_id}/scoreboard/snapshots", "post"),
        ("/api/admin/contests/{contest_id}/scoreboard/snapshots/latest", "get"),
        ("/api/contests/{contest_id}/print-requests", "post"),
        ("/api/contests/{contest_id}/print-requests/mine", "get"),
        ("/api/contests/{contest_id}/print-requests/all", "get"),
        ("/api/print-requests/{id}/pdf", "get"),
        ("/api/print-requests/{id}/retry", "post"),
        ("/api/print-requests/{id}/cancel", "post"),
        ("/api/print-requests/{id}/reject", "post"),
        ("/api/contests/{contest_id}/balloons", "get"),
        ("/api/contests/{contest_id}/balloons/stats", "get"),
        ("/api/balloons/{id}/claim", "post"),
        ("/api/balloons/{id}/deliver", "post"),
        ("/api/balloons/{id}/cancel", "post"),
        ("/api/balloons/{id}/reopen", "post"),
        ("/api/balloons/{id}/note", "patch"),
        ("/api/contests/{contest_id}/clarifications", "post"),
        ("/api/contests/{contest_id}/clarifications/mine", "get"),
        ("/api/contests/{contest_id}/clarifications/all", "get"),
        ("/api/clarifications/{id}", "get"),
        ("/api/clarifications/{id}/reply", "post"),
        ("/api/clarifications/{id}/close", "post"),
        ("/api/clarifications/{id}/convert", "post"),
        ("/api/admin/contests/{contest_id}/resolver-runs", "get"),
        ("/api/admin/contests/{contest_id}/resolver-runs", "post"),
        ("/api/admin/contests/{contest_id}/resolver-sources", "get"),
        ("/api/admin/resolver-runs/{id}", "get"),
        ("/api/admin/resolver-runs/{id}/events", "get"),
        ("/api/public/resolver-runs/{id}/state", "get"),
        ("/api/admin/resolver-runs/{id}/start", "post"),
        ("/api/admin/resolver-runs/{id}/next", "post"),
        ("/api/admin/resolver-runs/{id}/previous", "post"),
        ("/api/admin/resolver-runs/{id}/pause", "post"),
        ("/api/admin/resolver-runs/{id}/resume", "post"),
        ("/api/admin/resolver-runs/{id}/complete", "post"),
        ("/api/admin/resolver-runs/{id}/auto-play", "post"),
        ("/api/presentation-configs/{contest_id}", "get"),
        ("/api/presentation-configs/{contest_id}/screen", "put"),
        ("/api/presentation-configs/{contest_id}/live", "put"),
        ("/api/public/presentations/{contest_id}", "get"),
        ("/api/public/presentations/{contest_id}/metrics", "get"),
        ("/api/presentation-configs/{contest_id}/live/tokens", "get"),
        ("/api/presentation-configs/{contest_id}/live/tokens", "post"),
        ("/api/presentation-configs/{contest_id}/live/tokens/{token_id}", "delete"),
        ("/api/public/screens/register", "post"),
        ("/api/public/screens/{instance_id}/heartbeat", "post"),
        ("/api/screen-instances/{contest_id}", "get"),
        ("/api/screen-instances/{contest_id}/{instance_id}/commands", "post"),
        ("/api/screen-instances/{contest_id}/{instance_id}", "delete"),
        ("/api/contests/{contest_id}/screen-playlists", "get"),
        ("/api/contests/{contest_id}/screen-playlists", "post"),
        ("/api/screen-playlists/{playlist_id}", "put"),
        ("/api/screen-playlists/{playlist_id}", "delete"),
        ("/api/contests/{contest_id}/screen-groups", "get"),
        ("/api/contests/{contest_id}/screen-groups", "post"),
        ("/api/screen-groups/{group_id}", "put"),
        ("/api/screen-groups/{group_id}", "delete"),
        ("/api/screen-groups/{group_id}/control", "post"),
        ("/api/admin/contests/{contest_id}/award-categories", "get"),
        ("/api/admin/contests/{contest_id}/award-categories", "post"),
        ("/api/admin/award-categories/{id}", "put"),
        ("/api/admin/award-categories/{id}", "delete"),
        ("/api/admin/contests/{contest_id}/awards", "get"),
        ("/api/admin/contests/{contest_id}/awards", "post"),
        ("/api/admin/contests/{contest_id}/awards/resolver-runs", "get"),
        ("/api/admin/contests/{contest_id}/awards/candidates", "get"),
        ("/api/admin/contests/{contest_id}/awards/manual", "post"),
        ("/api/admin/award-recipients/{id}", "delete"),
        ("/api/admin/contests/{contest_id}/awards/freeze", "post"),
        ("/api/admin/contests/{contest_id}/awards/unfreeze", "post"),
        ("/api/admin/contests/{contest_id}/awards.csv", "get"),
        ("/api/public/contests/{contest_id}/awards/presentation", "get"),
        ("/api/contests/{contest_id}/awards/presentation", "put"),
        ("/api/contests/{contest_id}/awards/host-script", "get"),
        ("/api/contests/{contest_id}/awards/host-script", "put"),
        ("/api/contests/{contest_id}/awards/certificates/export", "get"),
        ("/api/contests/{contest_id}/announcements", "get"),
        ("/api/contests/{contest_id}/announcements", "post"),
        ("/api/admin/contests/{contest_id}/judge-queue/status", "get"),
        ("/api/admin/contests/{contest_id}/exports/tasks", "post"),
        ("/api/admin/contests/{contest_id}/exports/tasks/{task_id}", "get"),
        ("/api/admin/contests/{contest_id}/exports/tasks/{task_id}/download", "get"),
        ("/api/admin/staff-accounts", "get"),
        ("/api/admin/staff-accounts", "post"),
        ("/api/admin/staff-accounts/{user_id}", "patch"),
        ("/api/admin/staff-accounts/{user_id}/reset-password", "post"),
        ("/api/admin/contest-managers", "get"),
        ("/api/admin/contest-managers/{user_id}/contests", "put"),
        ("/api/admin/audit-logs", "get"),
        ("/api/public/events/contests/{contest_id}", "get"),
        ("/api/events/contests/{contest_id}", "get"),
        ("/api/team/events/contests/{contest_id}", "get"),
    ];

    /// Operations whose `security` is exactly one `session_cookie` requirement.
    const SESSION_SECURED_OPERATIONS: &[(&str, &str)] = &[
        ("/api/contests/{contest_id}/submissions", "get"),
        ("/api/contests/{contest_id}/submissions", "post"),
        ("/api/contests/{contest_id}/submissions/{submission_id}", "get"),
        ("/api/admin/contests/{contest_id}/submissions", "get"),
        ("/api/admin/contests/{contest_id}/submissions/{submission_id}", "get"),
        ("/api/admin/contests/{contest_id}/submissions/{submission_id}/rejudge", "post"),
        ("/api/admin/contests/{contest_id}/exports/submissions.csv", "get"),
        ("/api/admin/contests/{contest_id}/exports/submission-sources.zip", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/preview", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}", "get"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/pause", "post"),
        ("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/resume", "post"),
        ("/api/admin/contests/{contest_id}/scoreboard", "get"),
        ("/api/admin/contests/{contest_id}/scoreboard.csv", "get"),
        ("/api/admin/contests/{contest_id}/scoreboard/snapshots", "post"),
        ("/api/admin/contests/{contest_id}/scoreboard/snapshots/latest", "get"),
        ("/api/contests/{contest_id}/print-requests", "post"),
        ("/api/contests/{contest_id}/print-requests/mine", "get"),
        ("/api/contests/{contest_id}/print-requests/all", "get"),
        ("/api/print-requests/{id}/pdf", "get"),
        ("/api/print-requests/{id}/retry", "post"),
        ("/api/print-requests/{id}/cancel", "post"),
        ("/api/print-requests/{id}/reject", "post"),
        ("/api/contests/{contest_id}/balloons", "get"),
        ("/api/contests/{contest_id}/balloons/stats", "get"),
        ("/api/balloons/{id}/claim", "post"),
        ("/api/balloons/{id}/deliver", "post"),
        ("/api/balloons/{id}/cancel", "post"),
        ("/api/balloons/{id}/reopen", "post"),
        ("/api/balloons/{id}/note", "patch"),
        ("/api/contests/{contest_id}/clarifications", "post"),
        ("/api/contests/{contest_id}/clarifications/mine", "get"),
        ("/api/contests/{contest_id}/clarifications/all", "get"),
        ("/api/clarifications/{id}", "get"),
        ("/api/clarifications/{id}/reply", "post"),
        ("/api/clarifications/{id}/close", "post"),
        ("/api/clarifications/{id}/convert", "post"),
        ("/api/admin/staff-accounts", "get"),
        ("/api/admin/contest-managers", "get"),
        ("/api/admin/audit-logs", "get"),
        ("/api/events/contests/{contest_id}", "get"),
        ("/api/team/events/contests/{contest_id}", "get"),
        ("/api/teams", "get"),
        ("/api/teams/{team_id}", "get"),
        ("/api/teams/{team_id}/members", "get"),
        ("/api/problems", "get"),
        ("/api/problems/{problem_id}", "get"),
        ("/api/problems/{problem_id}/statements", "get"),
        ("/api/problems/{problem_id}/attachments", "get"),
        ("/api/problems/{problem_id}/attachments/{attachment_id}", "get"),
        ("/api/problems/{problem_id}/testdata", "get"),
        ("/api/problems/{problem_id}/testdata/versions", "get"),
        ("/api/problems/{problem_id}/testdata/versions/{version}", "get"),
        ("/api/contests/{contest_id}/problems", "get"),
    ];

    /// Operations whose `security` requires the session cookie plus both CSRF
    /// defenses (cookie and header).
    const MUTATION_SECURED_OPERATIONS: &[(&str, &str)] = &[
        ("/api/admin/staff-accounts", "post"),
        ("/api/admin/staff-accounts/{user_id}", "patch"),
        ("/api/admin/staff-accounts/{user_id}/reset-password", "post"),
        ("/api/admin/contest-managers/{user_id}/contests", "put"),
        ("/api/auth/logout", "post"),
        ("/api/auth/password", "post"),
        ("/api/contests", "post"),
        ("/api/contests/{contest_id}", "patch"),
        ("/api/contests/{contest_id}", "delete"),
        ("/api/contests/{contest_id}/transitions", "post"),
        ("/api/contests/{contest_id}/extensions", "post"),
        ("/api/contests/{source_contest_id}/clones", "post"),
        ("/api/teams", "post"),
        ("/api/teams/{team_id}", "patch"),
        ("/api/teams/{team_id}", "delete"),
        ("/api/teams/batch", "post"),
        ("/api/teams/{team_id}/members", "post"),
        ("/api/teams/{team_id}/members/{member_id}", "patch"),
        ("/api/teams/{team_id}/members/{member_id}", "delete"),
        ("/api/teams/{team_id}/account/reset-password", "post"),
        ("/api/contests/{contest_id}/teams", "post"),
        ("/api/contests/{contest_id}/teams/{team_id}", "delete"),
        ("/api/problems", "post"),
        ("/api/problems/{problem_id}", "patch"),
        ("/api/problems/{problem_id}", "delete"),
        ("/api/problems/{problem_id}/statements/{lang_code}", "put"),
        ("/api/problems/{problem_id}/statements/{lang_code}", "delete"),
        ("/api/problems/{problem_id}/attachments", "post"),
        ("/api/problems/{problem_id}/attachments/{attachment_id}", "delete"),
        ("/api/problems/{problem_id}/testdata", "post"),
        ("/api/problems/{problem_id}/testdata/versions/{version}/activate", "post"),
        ("/api/contests/{contest_id}/problems", "post"),
        ("/api/contests/{contest_id}/problems/{problem_id}", "patch"),
        ("/api/contests/{contest_id}/problems/{problem_id}", "delete"),
        ("/api/contests/{contest_id}/problems/reorder", "put"),
        ("/api/contests/{contest_id}/announcements", "post"),
    ];

    /// Operations that additionally require the `X-XSRF-TOKEN` header but
    /// whose full security object is otherwise covered elsewhere.
    const CSRF_HEADER_SECURED_OPERATIONS: &[(&str, &str)] = &[
        ("/api/contests/{contest_id}/print-requests", "post"),
        ("/api/print-requests/{id}/retry", "post"),
        ("/api/print-requests/{id}/cancel", "post"),
        ("/api/print-requests/{id}/reject", "post"),
        ("/api/balloons/{id}/claim", "post"),
        ("/api/balloons/{id}/deliver", "post"),
        ("/api/balloons/{id}/cancel", "post"),
        ("/api/balloons/{id}/reopen", "post"),
        ("/api/balloons/{id}/note", "patch"),
        ("/api/contests/{contest_id}/clarifications", "post"),
        ("/api/clarifications/{id}/reply", "post"),
        ("/api/clarifications/{id}/close", "post"),
        ("/api/clarifications/{id}/convert", "post"),
        ("/api/admin/contests/{contest_id}/scoreboard/snapshots", "post"),
        ("/api/admin/resolver-runs/{id}/start", "post"),
        ("/api/public/screens/register", "post"),
        ("/api/admin/contests/{contest_id}/awards", "post"),
    ];

    #[test]
    fn generated_contract_contains_documented_rust_operations() {
        let document = serde_json::to_value(document()).expect("serialize OpenAPI contract");

        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(document["info"]["title"], "ProjectBalloon API");
        let http_methods = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];
        let operation_count = document["paths"]
            .as_object()
            .expect("paths object")
            .values()
            .map(|item| {
                item.as_object()
                    .expect("path item")
                    .keys()
                    .filter(|key| http_methods.contains(&key.as_str()))
                    .count()
            })
            .sum::<usize>();
        // Update this snapshot count when a documented endpoint is intentionally added or removed.
        assert_eq!(operation_count, 203);

        for (path, method) in EXPECTED_OPERATIONS {
            assert!(
                document["paths"][path][method].is_object(),
                "contract is missing {method} {path}"
            );
        }
    }

    #[test]
    fn generated_contract_documents_session_and_csrf_requirements() {
        let document = serde_json::to_value(document()).expect("serialize OpenAPI contract");

        for (path, method) in SESSION_SECURED_OPERATIONS {
            assert!(
                document["paths"][path][method]["security"][0]["session_cookie"].is_array(),
                "expected session_cookie security for {method} {path}"
            );
        }
        for (path, method) in MUTATION_SECURED_OPERATIONS {
            let security = &document["paths"][path][method]["security"][0];
            assert!(
                security["session_cookie"].is_array(),
                "missing session_cookie for {method} {path}"
            );
            assert!(security["csrf_cookie"].is_array(), "missing csrf_cookie for {method} {path}");
            assert!(security["csrf_header"].is_array(), "missing csrf_header for {method} {path}");
        }
        for (path, method) in CSRF_HEADER_SECURED_OPERATIONS {
            assert!(
                document["paths"][path][method]["security"][0]["csrf_header"].is_array(),
                "expected csrf_header security for {method} {path}"
            );
        }
        assert!(
            document["paths"]["/api/contests/{contest_id}/submissions"]["post"]["security"][0]
                ["csrf_cookie"]
                .is_array()
        );
        let export_task_security = &document["paths"]["/api/admin/contests/{contest_id}/exports/tasks"]
            ["post"]["security"][0];
        assert!(export_task_security["session_cookie"].is_array());
        assert!(export_task_security["csrf_cookie"].is_array());
        assert!(export_task_security["csrf_header"].is_array());
        let login_security = &document["paths"]["/api/auth/login"]["post"]["security"][0];
        assert!(login_security.get("session_cookie").is_none());
        assert!(login_security["csrf_cookie"].is_array());
        assert!(login_security["csrf_header"].is_array());
        let current_user_security = &document["paths"]["/api/auth/me"]["get"]["security"][0];
        assert!(current_user_security["session_cookie"].is_array());
        assert!(current_user_security.get("csrf_cookie").is_none());
        assert!(current_user_security.get("csrf_header").is_none());
        assert!(document["paths"]["/api/auth/csrf"]["get"]["security"].is_null());
    }

    #[test]
    fn generated_contract_documents_anonymous_and_alternative_security() {
        let document = serde_json::to_value(document()).expect("serialize OpenAPI contract");

        assert!(
            document["paths"]["/api/contests/{contest_id}/scoreboard"]["get"]["security"][0]
                .as_object()
                .expect("anonymous scoreboard security")
                .is_empty()
        );
        assert!(
            document["paths"]["/api/contests/{contest_id}/scoreboard.csv"]["get"]["security"][0]
                .as_object()
                .expect("anonymous scoreboard csv security")
                .is_empty()
        );
        assert!(
            document["paths"]["/api/public/resolver-runs/{id}/state"]["get"]["security"].is_null()
        );
        assert!(
            document["paths"]["/api/public/contests/{contest_id}/awards/presentation"]["get"]
                ["security"]
                .is_null()
        );
        assert!(
            document["paths"]["/api/public/events/contests/{contest_id}"]["get"]["security"]
                .is_null()
        );
        assert!(
            document["paths"]["/api/public/presentations/{contest_id}"]["get"]["security"][1]
                ["broadcast_token_header"]
                .is_array()
        );
        assert!(document["components"]["securitySchemes"]["broadcast_token_query"].is_null());
        assert!(document["components"]["securitySchemes"]["session_cookie"].is_object());
        assert!(document["components"]["securitySchemes"]["csrf_cookie"].is_object());
        assert!(document["components"]["securitySchemes"]["csrf_header"].is_object());
        assert_eq!(document["components"]["securitySchemes"]["csrf_cookie"]["name"], "XSRF-TOKEN");
        assert_eq!(
            document["components"]["securitySchemes"]["csrf_header"]["name"],
            "X-XSRF-TOKEN"
        );
        for path in ["/api/contests", "/api/contests/{contest_id}"] {
            let security = &document["paths"][path]["get"]["security"];
            assert_eq!(security.as_array().expect("optional contest security").len(), 2);
            assert_eq!(security[0].as_object().expect("anonymous security").len(), 0);
            assert!(security[1]["session_cookie"].is_array());
        }
        let contest_teams_security =
            &document["paths"]["/api/contests/{contest_id}/teams"]["get"]["security"];
        assert_eq!(contest_teams_security.as_array().expect("optional roster security").len(), 2);
        assert_eq!(contest_teams_security[0].as_object().expect("anonymous security").len(), 0);
    }

    #[test]
    fn generated_contract_exposes_response_content_and_schema_formats() {
        let document = serde_json::to_value(document()).expect("serialize OpenAPI contract");

        assert!(document["paths"]["/api/admin/contests/{contest_id}/scoreboard.csv"]["get"]["responses"]["200"]["content"]["text/csv"].is_object());
        assert!(
            document["paths"]["/api/print-requests/{id}/pdf"]["get"]["responses"]["200"]["content"]
                ["application/pdf"]
                .is_object()
        );
        assert!(document["paths"]["/api/admin/contests/{contest_id}/exports/submission-sources.zip"]["get"]["responses"]["200"]["content"]["application/zip"].is_object());
        assert!(
            document["paths"]["/api/auth/login"]["post"]["responses"]["429"]["content"]["application/json"]
                ["schema"]["$ref"]
                == "#/components/schemas/ApiErrorBody"
        );
        assert!(document["paths"]["/api/public/events/contests/{contest_id}"]["get"]["responses"]["200"]["content"]["text/event-stream"].is_object());
        assert!(document["paths"]["/api/admin/contest-admins"].is_null());
        let date_time_fields = [
            ("AnnouncementResponse", "createdAt"),
            ("ContestResponse", "createdAt"),
            ("ContestExtensionRequest", "expectedEndAt"),
            ("TeamMemberResponse", "createdAt"),
            ("ProblemResponse", "createdAt"),
            ("SubmissionSummary", "submittedAt"),
            ("ScoreboardResponse", "generatedAt"),
            ("ResolverRunResponse", "createdAt"),
            ("PresentationResponse", "serverTime"),
        ];
        for (schema, field) in date_time_fields {
            assert_eq!(
                document["components"]["schemas"][schema]["properties"][field]["format"],
                "date-time",
                "expected date-time format on {schema}.{field}"
            );
        }
        assert_eq!(
            document["components"]["schemas"]["LoginRequest"]["properties"]["password"]["writeOnly"],
            true
        );
        assert_eq!(
            document["components"]["schemas"]["CurrentUserResponse"]["properties"]["userType"]["$ref"],
            "#/components/schemas/UserType"
        );
        assert!(
            document["components"]["schemas"]["CurrentUserResponse"]["properties"]["permissions"]
                .is_object()
        );
        assert!(
            document["components"]["schemas"]["CurrentUserResponse"]["properties"]["roles"]
                .is_null()
        );
        let page_schema_ref = document["paths"]["/api/contests"]["get"]["responses"]["200"]
            ["content"]["application/json"]["schema"]["$ref"]
            .as_str()
            .expect("contest page schema reference");
        let page_schema_name =
            page_schema_ref.strip_prefix("#/components/schemas/").expect("component reference");
        assert_eq!(
            document["components"]["schemas"][page_schema_name]["properties"]["content"]["items"]["properties"]
                ["status"]["$ref"],
            "#/components/schemas/ContestStatus"
        );
    }

    #[test]
    fn generated_contract_has_unique_operation_ids_and_valid_path_parameters() {
        let document = serde_json::to_value(document()).expect("serialize OpenAPI contract");
        let paths = document["paths"].as_object().expect("paths object");
        let mut operation_ids = HashSet::new();

        for (path, item) in paths {
            for operation in item.as_object().expect("path item").values() {
                let operation_id = operation["operationId"].as_str().expect("operation id");
                assert!(
                    operation_ids.insert(operation_id),
                    "duplicate operation id: {operation_id}"
                );

                for parameter in operation["parameters"].as_array().into_iter().flatten() {
                    if parameter["in"] == Value::String("path".to_owned()) {
                        let name = parameter["name"].as_str().expect("path parameter name");
                        assert!(
                            path.contains(&format!("{{{name}}}")),
                            "{path} is missing {{{name}}}"
                        );
                        assert_eq!(parameter["required"], true);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn swagger_router_serves_the_generated_contract() {
        let app: Router = Router::new().merge(swagger_ui());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/openapi.json")
                    .body(Body::empty())
                    .expect("OpenAPI request"),
            )
            .await
            .expect("serve OpenAPI contract");

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.expect("read OpenAPI body").to_bytes();
        let document: Value = serde_json::from_slice(&body).expect("parse OpenAPI response");
        assert_eq!(document["info"]["title"], "ProjectBalloon API");
    }
}
