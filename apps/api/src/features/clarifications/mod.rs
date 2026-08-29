mod handlers;
mod model;
mod service;

pub use handlers::*;
pub use model::*;
pub use service::ClarificationService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/clarifications", axum::routing::post(ask))
        .route("/api/contests/{contest_id}/clarifications/mine", axum::routing::get(list_mine))
        .route("/api/contests/{contest_id}/clarifications/all", axum::routing::get(list_all))
        .route("/api/clarifications/{id}", axum::routing::get(get))
        .route("/api/clarifications/{id}/reply", axum::routing::post(reply))
        .route("/api/clarifications/{id}/close", axum::routing::post(close))
        .route("/api/clarifications/{id}/convert", axum::routing::post(convert))
}

#[cfg(test)]
mod tests;
