mod handlers;
mod model;
mod service;

pub use handlers::*;
pub use model::*;
pub use service::BalloonService;
pub(crate) use service::generate_for_accepted;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/balloons", get(list))
        .route("/api/contests/{contest_id}/balloons/stats", get(stats))
        .route(
            "/api/contests/{contest_id}/balloons/dispatch-policy",
            get(dispatch_policy).put(update_dispatch_policy),
        )
        .route("/api/contests/{contest_id}/balloons/dispatch", post(dispatch))
        .route("/api/balloons/{id}/claim", post(claim))
        .route("/api/balloons/{id}/deliver", post(deliver))
        .route("/api/balloons/{id}/cancel", post(cancel))
        .route("/api/balloons/{id}/reopen", post(reopen))
        .route("/api/balloons/{id}/note", patch(note))
}

#[cfg(test)]
mod tests;

use axum::routing::{get, patch, post};
