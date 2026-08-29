mod dispatcher;
mod fanout;
pub(crate) mod handlers;
mod hub;

pub use dispatcher::{DispatcherConfig, OutboxDispatcher};
pub use fanout::{RealtimePublisher, RedisSubscriber};
pub use handlers::{subscribe_public, subscribe_staff, subscribe_team};
pub use hub::RealtimeHub;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/public/events/contests/{contest_id}", get(subscribe_public))
        .route("/api/events/contests/{contest_id}", get(subscribe_staff))
        .route("/api/team/events/contests/{contest_id}", get(subscribe_team))
}

use axum::routing::get;
