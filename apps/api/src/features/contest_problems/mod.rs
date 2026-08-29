pub(crate) mod handlers;
mod model;
mod service;

pub use handlers::{assign, list, remove, reorder, update};
pub use service::ContestProblemService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/problems", get(list).post(assign))
        .route("/api/contests/{contest_id}/problems/{problem_id}", patch(update).delete(remove))
        .route("/api/contests/{contest_id}/problems/reorder", put(reorder))
}

use axum::routing::{get, patch, put};
