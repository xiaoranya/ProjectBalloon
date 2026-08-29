macro_rules! safe_sql {
    ($($arg:tt)*) => {
        sqlx::AssertSqlSafe(format!($($arg)*))
    };
}

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
    extract::{ConnectInfo, State},
    http::Request,
    middleware,
};

use crate::{
    features::{
        announcements, audit_logs, auth, awards, balloons, clarifications, competition,
        contest_management_scopes, contest_problems, contests, presentation, printing, problems,
        realtime, resolver, scoreboard, scoring, staff_accounts, submissions, teams, training,
        virtual_practice,
    },
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

async fn enforce_deployment_mode(
    State(state): State<AppState>,
    request: Request<Body>,
    next: middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = request.uri().path();
    let daily = path == "/api/auth/register"
        || path.starts_with("/api/public/problem-bank")
        || path.starts_with("/api/practice")
        || path.starts_with("/api/training")
        || path.starts_with("/api/admin/practice")
        || path.starts_with("/api/admin/training")
        || (path.starts_with("/api/admin/problems/")
            && (path.ends_with("/publication") || path.contains("/editorials/")));
    if state.deployment_mode().is_competition() && daily {
        return crate::error::AppError::not_found(
            "DAILY_FEATURE_DISABLED",
            "This feature is disabled in competition mode",
        )
        .into_response();
    }
    next.run(request).await
}

pub const SERVICE_NAME: &str = "ProjectBalloon";

pub fn router(state: AppState, trusted_proxy_cidrs: Vec<IpNet>) -> Router {
    Router::new()
        .merge(openapi::swagger_ui())
        .merge(health::routes())
        .merge(competition::routes())
        .merge(training::routes())
        .merge(metrics::routes())
        .merge(auth::routes())
        .merge(submissions::routes())
        .merge(virtual_practice::routes())
        .merge(clarifications::routes())
        .merge(announcements::routes())
        .merge(printing::routes())
        .merge(balloons::routes())
        .merge(resolver::routes())
        .merge(awards::routes())
        .merge(presentation::routes())
        .merge(staff_accounts::routes())
        .merge(contest_management_scopes::routes())
        .merge(audit_logs::routes())
        .merge(contests::routes())
        .merge(scoreboard::routes())
        .merge(scoring::routes())
        .merge(contest_problems::routes())
        .merge(problems::routes())
        .merge(teams::routes())
        .merge(realtime::routes())
        .layer(middleware::from_fn_with_state(state.clone(), auth::protect_csrf))
        .layer(middleware::from_fn_with_state(trusted_proxy_cidrs, apply_forwarded_client_ip))
        .layer(middleware::from_fn_with_state(state.clone(), enforce_deployment_mode))
        .with_state(state)
}
