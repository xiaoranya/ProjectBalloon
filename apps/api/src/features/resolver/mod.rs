mod handlers;
mod model;
mod plan;
mod runner;
mod service;
/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route(
            "/api/admin/contests/{contest_id}/resolver-runs",
            axum::routing::get(list).post(create),
        )
        .route("/api/admin/contests/{contest_id}/resolver-sources", axum::routing::get(sources))
        .route("/api/admin/resolver-runs/{id}", axum::routing::get(get))
        .route("/api/admin/resolver-runs/{id}/events", axum::routing::get(events))
        .route("/api/public/resolver-runs/{id}/state", axum::routing::get(public_state))
        .route("/api/admin/resolver-runs/{id}/start", axum::routing::post(start))
        .route("/api/admin/resolver-runs/{id}/next", axum::routing::post(next))
        .route("/api/admin/resolver-runs/{id}/previous", axum::routing::post(previous))
        .route("/api/admin/resolver-runs/{id}/pause", axum::routing::post(pause))
        .route("/api/admin/resolver-runs/{id}/resume", axum::routing::post(resume))
        .route("/api/admin/resolver-runs/{id}/complete", axum::routing::post(complete))
        .route("/api/admin/resolver-runs/{id}/auto-play", axum::routing::post(auto_play))
}

#[cfg(test)]
mod tests;

pub use handlers::*;
pub use model::{
    AutoPlayRequest, CommandRequest, CreateRequest, ResolverEventResponse,
    ResolverPublicStateResponse, ResolverRunResponse, ResolverSourceSnapshotResponse,
    ResolverSourcesResponse,
};
pub use runner::ResolverAutoRunner;
pub use service::ResolverService;
