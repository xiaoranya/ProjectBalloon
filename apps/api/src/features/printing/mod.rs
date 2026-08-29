mod delivery;
pub use delivery::{CommandLineCupsGateway, CupsDeliveryRunner, CupsGateway, CupsJobStatus};

mod handlers;
mod model;
mod service;

pub use handlers::*;
pub use model::*;
pub use service::PrintingService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/print-requests", post(create))
        .route("/api/contests/{contest_id}/print-requests/mine", get(list_mine))
        .route("/api/contests/{contest_id}/print-requests/all", get(list_all))
        .route("/api/print-requests/{id}/pdf", get(download_pdf))
        .route("/api/print-requests/{id}/retry", post(retry))
        .route("/api/print-requests/{id}/cancel", post(cancel))
        .route("/api/print-requests/{id}/reject", post(reject))
}

#[cfg(test)]
mod tests;

use axum::routing::{get, post};
