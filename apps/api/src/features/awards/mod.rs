use std::net::SocketAddr;

use crate::{error::AppError, features::auth::AuthContext, state::AppState};
use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

mod handlers;
mod model;
mod service;
/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route(
            "/api/admin/contests/{contest_id}/award-categories",
            axum::routing::get(list_categories).post(create_category),
        )
        .route(
            "/api/admin/award-categories/{id}",
            axum::routing::put(update_category).delete(delete_category),
        )
        .route(
            "/api/admin/contests/{contest_id}/awards",
            axum::routing::get(handlers::get).post(generate),
        )
        .route(
            "/api/admin/contests/{contest_id}/awards/resolver-runs",
            axum::routing::get(completed_resolver_runs),
        )
        .route("/api/admin/contests/{contest_id}/awards/candidates", axum::routing::get(candidates))
        .route("/api/admin/contests/{contest_id}/awards/manual", axum::routing::post(manual_add))
        .route("/api/admin/award-recipients/{id}", axum::routing::delete(manual_remove))
        .route("/api/admin/contests/{contest_id}/awards/freeze", axum::routing::post(freeze))
        .route("/api/admin/contests/{contest_id}/awards/unfreeze", axum::routing::post(unfreeze))
        .route("/api/admin/contests/{contest_id}/awards.csv", axum::routing::get(csv))
        .route(
            "/api/public/contests/{contest_id}/awards/presentation",
            axum::routing::get(public_presentation),
        )
        .route(
            "/api/contests/{contest_id}/awards/presentation",
            axum::routing::put(update_presentation),
        )
        .route(
            "/api/contests/{contest_id}/awards/host-script",
            axum::routing::get(get_host_script).put(save_host_script),
        )
        .route(
            "/api/contests/{contest_id}/awards/certificates/export",
            axum::routing::get(certificate_export),
        )
}

#[cfg(test)]
mod tests;

pub use handlers::*;
pub use model::*;
pub use service::AwardService;
