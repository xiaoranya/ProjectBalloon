pub mod bootstrap;
pub mod config;
pub mod error;
pub mod features;
mod health;
mod metrics;
pub mod object_storage;
pub mod object_storage_cleanup;
pub mod openapi;
mod pagination;
pub mod state;

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, State},
    http::Request,
    middleware,
    routing::{delete, get, patch, post, put},
};

use crate::{
    features::{
        announcements, audit_logs, auth, awards, balloons, clarifications, contest_admin_scopes,
        contest_problems, contests, presentation, printing, problems, realtime, resolver,
        scoreboard, staff_accounts, submissions, teams,
    },
    health::{liveness, readiness},
    state::AppState,
};
use ipnet::IpNet;

async fn apply_forwarded_client_ip(
    State(trusted_proxy_cidrs): State<Vec<IpNet>>,
    mut request: Request<Body>,
    next: middleware::Next,
) -> axum::response::Response {
    // Never accept a forwarding header from an arbitrary peer. Deployments must
    // explicitly configure the CIDRs in which their reverse proxies run.
    if let Some(peer) = request.extensions().get::<ConnectInfo<std::net::SocketAddr>>().copied()
        && trusted_proxy_cidrs.iter().any(|cidr| cidr.contains(&peer.0.ip()))
        && let Some(value) =
            request.headers().get("x-real-ip").and_then(|value| value.to_str().ok())
        && let Ok(ip) = value.parse::<std::net::IpAddr>()
    {
        request.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::new(ip, peer.0.port())));
    }
    next.run(request).await
}

pub const SERVICE_NAME: &str = "xcpc-platform";

pub fn router(state: AppState, trusted_proxy_cidrs: Vec<IpNet>) -> Router {
    Router::new()
        .merge(openapi::swagger_ui())
        .route("/livez", get(liveness))
        .route("/api/health", get(readiness))
        .route("/metrics", get(metrics::prometheus))
        .route("/api/auth/csrf", get(auth::csrf))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::current_user))
        .route("/api/auth/password", post(auth::change_password))
        .route("/api/contests/{contest_id}/clarifications", post(clarifications::ask))
        .route("/api/contests/{contest_id}/clarifications/mine", get(clarifications::list_mine))
        .route("/api/contests/{contest_id}/clarifications/all", get(clarifications::list_all))
        .route("/api/clarifications/{id}", get(clarifications::get))
        .route("/api/clarifications/{id}/reply", post(clarifications::reply))
        .route("/api/clarifications/{id}/close", post(clarifications::close))
        .route("/api/clarifications/{id}/convert", post(clarifications::convert))
        .route(
            "/api/contests/{contest_id}/announcements",
            get(announcements::list).post(announcements::create),
        )
        .route("/api/announcements/{id}", get(announcements::get).patch(announcements::update))
        .route("/api/announcements/{id}/pin", post(announcements::pin))
        .route("/api/announcements/{id}/schedule", post(announcements::schedule))
        .route("/api/announcements/{id}/cancel", post(announcements::cancel))
        .route("/api/announcements/{id}/withdraw", post(announcements::withdraw))
        .route("/api/contests/{contest_id}/print-requests", post(printing::create))
        .route("/api/contests/{contest_id}/print-requests/mine", get(printing::list_mine))
        .route("/api/contests/{contest_id}/print-requests/all", get(printing::list_all))
        .route("/api/print-requests/{id}/pdf", get(printing::download_pdf))
        .route("/api/print-requests/{id}/retry", post(printing::retry))
        .route("/api/print-requests/{id}/cancel", post(printing::cancel))
        .route("/api/print-requests/{id}/reject", post(printing::reject))
        .route("/api/contests/{contest_id}/balloons", get(balloons::list))
        .route("/api/contests/{contest_id}/balloons/stats", get(balloons::stats))
        .route("/api/balloons/{id}/claim", post(balloons::claim))
        .route("/api/balloons/{id}/deliver", post(balloons::deliver))
        .route("/api/balloons/{id}/cancel", post(balloons::cancel))
        .route("/api/balloons/{id}/reopen", post(balloons::reopen))
        .route("/api/balloons/{id}/note", patch(balloons::note))
        .route(
            "/api/admin/contests/{contest_id}/resolver-runs",
            get(resolver::list).post(resolver::create),
        )
        .route("/api/admin/contests/{contest_id}/resolver-sources", get(resolver::sources))
        .route("/api/admin/resolver-runs/{id}", get(resolver::get))
        .route("/api/admin/resolver-runs/{id}/events", get(resolver::events))
        .route("/api/public/resolver-runs/{id}/state", get(resolver::public_state))
        .route("/api/admin/resolver-runs/{id}/start", post(resolver::start))
        .route("/api/admin/resolver-runs/{id}/next", post(resolver::next))
        .route("/api/admin/resolver-runs/{id}/previous", post(resolver::previous))
        .route("/api/admin/resolver-runs/{id}/pause", post(resolver::pause))
        .route("/api/admin/resolver-runs/{id}/resume", post(resolver::resume))
        .route("/api/admin/resolver-runs/{id}/complete", post(resolver::complete))
        .route("/api/admin/resolver-runs/{id}/auto-play", post(resolver::auto_play))
        .route(
            "/api/admin/contests/{contest_id}/award-categories",
            get(awards::list_categories).post(awards::create_category),
        )
        .route(
            "/api/admin/award-categories/{id}",
            put(awards::update_category).delete(awards::delete_category),
        )
        .route("/api/admin/contests/{contest_id}/awards", get(awards::get).post(awards::generate))
        .route(
            "/api/admin/contests/{contest_id}/awards/resolver-runs",
            get(awards::completed_resolver_runs),
        )
        .route("/api/admin/contests/{contest_id}/awards/candidates", get(awards::candidates))
        .route("/api/admin/contests/{contest_id}/awards/manual", post(awards::manual_add))
        .route("/api/admin/award-recipients/{id}", delete(awards::manual_remove))
        .route("/api/admin/contests/{contest_id}/awards/freeze", post(awards::freeze))
        .route("/api/admin/contests/{contest_id}/awards/unfreeze", post(awards::unfreeze))
        .route("/api/admin/contests/{contest_id}/awards.csv", get(awards::csv))
        .route(
            "/api/public/contests/{contest_id}/awards/presentation",
            get(awards::public_presentation),
        )
        .route("/api/contests/{contest_id}/awards/presentation", put(awards::update_presentation))
        .route(
            "/api/contests/{contest_id}/awards/host-script",
            get(awards::get_host_script).put(awards::save_host_script),
        )
        .route(
            "/api/contests/{contest_id}/awards/certificates/export",
            get(awards::certificate_export),
        )
        .route("/api/presentation-configs/{contest_id}", get(presentation::get_config))
        .route("/api/presentation-configs/{contest_id}/screen", put(presentation::update_screen))
        .route("/api/presentation-configs/{contest_id}/live", put(presentation::update_live))
        .route("/api/public/presentations/{contest_id}", get(presentation::published))
        .route("/api/public/presentations/{contest_id}/metrics", get(presentation::metrics))
        .route(
            "/api/presentation-configs/{contest_id}/live/tokens",
            get(presentation::list_tokens).post(presentation::create_token),
        )
        .route(
            "/api/presentation-configs/{contest_id}/live/tokens/{token_id}",
            delete(presentation::revoke_token),
        )
        .route("/api/public/screens/register", post(presentation::register))
        .route("/api/public/screens/{instance_id}/heartbeat", post(presentation::heartbeat))
        .route("/api/screen-instances/{contest_id}", get(presentation::list_instances))
        .route(
            "/api/screen-instances/{contest_id}/{instance_id}/commands",
            post(presentation::command),
        )
        .route("/api/screen-instances/{contest_id}/{instance_id}", delete(presentation::revoke))
        .route(
            "/api/contests/{contest_id}/screen-playlists",
            get(presentation::list_playlists).post(presentation::create_playlist),
        )
        .route(
            "/api/screen-playlists/{playlist_id}",
            put(presentation::update_playlist).delete(presentation::delete_playlist),
        )
        .route(
            "/api/contests/{contest_id}/screen-groups",
            get(presentation::list_groups).post(presentation::create_group),
        )
        .route(
            "/api/screen-groups/{group_id}",
            put(presentation::update_group).delete(presentation::delete_group),
        )
        .route("/api/screen-groups/{group_id}/control", post(presentation::control_group))
        .route("/api/admin/staff-accounts", get(staff_accounts::list).post(staff_accounts::create))
        .route("/api/admin/staff-accounts/{user_id}", patch(staff_accounts::update))
        .route(
            "/api/admin/staff-accounts/{user_id}/reset-password",
            post(staff_accounts::reset_password),
        )
        .route("/api/admin/contest-admins", get(contest_admin_scopes::list))
        .route("/api/admin/contest-admins/{user_id}/contests", put(contest_admin_scopes::replace))
        .route("/api/admin/audit-logs", get(audit_logs::list))
        .route("/api/contests", get(contests::list).post(contests::create))
        .route(
            "/api/contests/{contest_id}",
            get(contests::get).patch(contests::update).delete(contests::delete),
        )
        .route("/api/contests/{contest_id}/transitions", post(contests::transition))
        .route("/api/contests/{contest_id}/clones", post(contests::clone_contest))
        .route("/api/contests/{contest_id}/extensions", post(contests::extend))
        .route("/api/contests/{contest_id}/scoreboard", get(scoreboard::public))
        .route("/api/contests/{contest_id}/scoreboard.csv", get(scoreboard::public_csv))
        .route("/api/admin/contests/{contest_id}/scoreboard", get(scoreboard::admin))
        .route("/api/admin/contests/{contest_id}/scoreboard.csv", get(scoreboard::admin_csv))
        .route(
            "/api/admin/contests/{contest_id}/scoreboard/snapshots",
            post(scoreboard::create_snapshot),
        )
        .route(
            "/api/admin/contests/{contest_id}/scoreboard/snapshots/latest",
            get(scoreboard::latest_snapshot),
        )
        .route(
            "/api/contests/{contest_id}/problems",
            get(contest_problems::list).post(contest_problems::assign),
        )
        .route(
            "/api/contests/{contest_id}/problems/{problem_id}",
            patch(contest_problems::update).delete(contest_problems::remove),
        )
        .route(
            "/api/contests/{contest_id}/submissions",
            get(submissions::list_own)
                .post(submissions::submit)
                .layer(DefaultBodyLimit::max(70 * 1024)),
        )
        .route(
            "/api/contests/{contest_id}/submissions/{submission_id}",
            get(submissions::detail_own),
        )
        .route("/api/admin/contests/{contest_id}/submissions", get(submissions::list_admin))
        .route(
            "/api/admin/contests/{contest_id}/judge-queue/status",
            get(submissions::judge_queue_status),
        )
        .route(
            "/api/admin/contests/{contest_id}/submissions/{submission_id}",
            get(submissions::detail_admin),
        )
        .route(
            "/api/admin/contests/{contest_id}/submissions/{submission_id}/rejudge",
            post(submissions::rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/exports/submissions.csv",
            get(submissions::export_metadata_csv),
        )
        .route(
            "/api/admin/contests/{contest_id}/exports/submission-sources.zip",
            get(submissions::export_sources_zip),
        )
        .route(
            "/api/admin/contests/{contest_id}/exports/tasks",
            post(submissions::create_export_task),
        )
        .route(
            "/api/admin/contests/{contest_id}/exports/tasks/{task_id}",
            get(submissions::get_export_task),
        )
        .route(
            "/api/admin/contests/{contest_id}/exports/tasks/{task_id}/download",
            get(submissions::download_export_task),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/preview",
            post(submissions::preview_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks",
            get(submissions::list_batch_rejudge).post(submissions::create_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}",
            get(submissions::get_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/pause",
            post(submissions::pause_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/resume",
            post(submissions::resume_batch_rejudge),
        )
        .route("/api/contests/{contest_id}/problems/reorder", put(contest_problems::reorder))
        .route("/api/problems", get(problems::list).post(problems::create))
        .route(
            "/api/problems/{problem_id}",
            get(problems::get).patch(problems::update).delete(problems::delete),
        )
        .route(
            "/api/problems/{problem_id}/statements/{lang_code}",
            put(problems::upsert_statement).delete(problems::delete_statement),
        )
        .route("/api/problems/{problem_id}/statements", get(problems::list_statements))
        .route(
            "/api/problems/{problem_id}/testdata",
            get(problems::download_testdata)
                .post(problems::upload_testdata)
                .layer(DefaultBodyLimit::max(258 * 1024 * 1024)),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions",
            get(problems::list_testdata_versions),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions/{version}",
            get(problems::download_testdata_version),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions/{version}/activate",
            post(problems::activate_testdata_version),
        )
        .route(
            "/api/problems/{problem_id}/attachments",
            get(problems::list_attachments)
                .post(problems::upload_attachment)
                .layer(DefaultBodyLimit::max(22 * 1024 * 1024)),
        )
        .route(
            "/api/problems/{problem_id}/attachments/{attachment_id}",
            get(problems::download_attachment).delete(problems::delete_attachment),
        )
        .route("/api/teams", get(teams::list).post(teams::create))
        .route("/api/teams/{team_id}", get(teams::get).patch(teams::update).delete(teams::delete))
        .route("/api/teams/batch", post(teams::batch_import))
        .route("/api/teams/{team_id}/members", get(teams::list_members).post(teams::add_member))
        .route(
            "/api/teams/{team_id}/members/{member_id}",
            patch(teams::update_member).delete(teams::remove_member),
        )
        .route("/api/teams/{team_id}/account/reset-password", post(teams::reset_password))
        .route(
            "/api/contests/{contest_id}/teams",
            get(teams::list_contest_teams).post(teams::assign_to_contest),
        )
        .route(
            "/api/contests/{contest_id}/teams/{team_id}",
            axum::routing::delete(teams::remove_from_contest),
        )
        .route("/api/public/events/contests/{contest_id}", get(realtime::subscribe_public))
        .route("/api/events/contests/{contest_id}", get(realtime::subscribe_staff))
        .route("/api/team/events/contests/{contest_id}", get(realtime::subscribe_team))
        .layer(middleware::from_fn_with_state(state.clone(), auth::protect_csrf))
        .layer(middleware::from_fn_with_state(trusted_proxy_cidrs, apply_forwarded_client_ip))
        .with_state(state)
}
