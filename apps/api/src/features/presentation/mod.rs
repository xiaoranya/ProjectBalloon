mod handlers;
mod model;
mod service;

mod orchestration;
pub use orchestration::*;
mod live;
pub use live::*;

pub use handlers::*;
pub use model::*;
pub use service::PresentationService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/presentation-configs/{contest_id}", get(get_config))
        .route("/api/presentation-templates", get(list_templates).post(create_template))
        .route("/api/presentation-templates/{template_id}", put(update_template))
        .route("/api/presentation-configs/{contest_id}/screen", put(update_screen))
        .route("/api/presentation-configs/{contest_id}/live", put(update_live))
        .route("/api/public/presentations/{contest_id}", get(published))
        .route("/api/public/presentations/{contest_id}/metrics", get(metrics))
        .route(
            "/api/presentation-configs/{contest_id}/live/tokens",
            get(list_tokens).post(create_token),
        )
        .route(
            "/api/presentation-configs/{contest_id}/live/tokens/{token_id}",
            delete(revoke_token),
        )
        .route("/api/public/screens/register", post(register))
        .route("/api/public/screens/{instance_id}/heartbeat", post(heartbeat))
        .route("/api/screen-instances/{contest_id}", get(list_instances))
        .route("/api/screen-instances/{contest_id}/{instance_id}/commands", post(command))
        .route("/api/screen-instances/{contest_id}/{instance_id}", delete(revoke))
        .route(
            "/api/contests/{contest_id}/screen-playlists",
            get(list_playlists).post(create_playlist),
        )
        .route("/api/screen-playlists/{playlist_id}", put(update_playlist).delete(delete_playlist))
        .route("/api/contests/{contest_id}/screen-groups", get(list_groups).post(create_group))
        .route("/api/screen-groups/{group_id}", put(update_group).delete(delete_group))
        .route("/api/screen-groups/{group_id}/control", post(control_group))
}

#[cfg(test)]
mod tests;

use axum::routing::{delete, get, post, put};
