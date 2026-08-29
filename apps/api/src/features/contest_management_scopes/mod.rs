pub(crate) mod handlers;
mod model;
mod service;

pub use handlers::{list, replace};
pub use service::ContestManagementScopeService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/admin/contest-managers", get(list))
        .route("/api/admin/contest-managers/{user_id}/contests", put(replace))
}

use axum::routing::{get, put};

#[cfg(test)]
mod tests;
