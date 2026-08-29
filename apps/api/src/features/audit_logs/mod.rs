pub(crate) mod handlers;
mod model;
mod service;

#[cfg(test)]
mod tests;

pub use handlers::list;
pub use service::AuditLogService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new().route("/api/admin/audit-logs", get(list))
}

use axum::routing::get;
