pub(crate) mod handlers;
mod helpers;
mod model;
mod service;

pub use handlers::{clone_contest, create, delete, extend, get, list, transition, update};
pub use model::ContestStatus;
pub use service::ContestService;
mod lifecycle_runner;
pub use lifecycle_runner::ContestLifecycleRunner;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests", axum::routing::get(list).post(create))
        .route("/api/contests/{contest_id}", axum::routing::get(get).patch(update).delete(delete))
        .route("/api/contests/{contest_id}/transitions", axum::routing::post(transition))
        .route("/api/contests/{contest_id}/clones", axum::routing::post(clone_contest))
        .route("/api/contests/{contest_id}/extensions", axum::routing::post(extend))
}

#[cfg(test)]
mod tests;
