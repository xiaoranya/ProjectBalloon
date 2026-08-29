mod handlers;
mod model;
mod runner;
mod service;
/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/announcements", axum::routing::get(list).post(create))
        .route("/api/announcements/{id}", axum::routing::get(get).patch(update))
        .route("/api/announcements/{id}/pin", axum::routing::post(pin))
        .route("/api/announcements/{id}/schedule", axum::routing::post(schedule))
        .route("/api/announcements/{id}/cancel", axum::routing::post(cancel))
        .route("/api/announcements/{id}/withdraw", axum::routing::post(withdraw))
}

#[cfg(test)]
mod tests;

pub use handlers::*;
pub use model::*;
pub use runner::AnnouncementScheduleRunner;
pub use service::AnnouncementService;
pub(crate) use service::{audit_tx, ensure_open_tx, load, public_event_tx, validate_text};
