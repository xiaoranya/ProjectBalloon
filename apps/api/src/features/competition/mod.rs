mod handlers;
pub mod model;
mod service;

pub use handlers::{
    bind, create_workstation, deployment, list_bindings, list_workstations, revoke, rotate,
    update_workstation,
};
pub use service::CompetitionService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/deployment", get(deployment))
        .route(
            "/api/admin/competition/workstations",
            get(list_workstations).post(create_workstation),
        )
        .route("/api/admin/competition/workstations/{id}", patch(update_workstation))
        .route(
            "/api/admin/contests/{contest_id}/workstation-bindings",
            get(list_bindings).post(bind),
        )
        .route(
            "/api/admin/contests/{contest_id}/workstation-bindings/{binding_id}/rotate",
            post(rotate),
        )
        .route("/api/admin/contests/{contest_id}/workstation-bindings/{binding_id}", delete(revoke))
}

use axum::routing::{delete, get, patch, post};
