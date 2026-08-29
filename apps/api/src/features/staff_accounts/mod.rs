pub(crate) mod handlers;
mod model;
mod service;

pub use handlers::{create, list, reset_password, update};
pub use service::StaffAccountService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/admin/staff-accounts", get(list).post(create))
        .route("/api/admin/staff-accounts/{user_id}", patch(update))
        .route("/api/admin/staff-accounts/{user_id}/reset-password", post(reset_password))
}

use axum::routing::{get, patch, post};
