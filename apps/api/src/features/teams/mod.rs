pub(crate) mod handlers;
mod model;
mod service;

pub use handlers::{
    add_member, assign_to_contest, batch_import, create, delete, get, list, list_contest_teams,
    list_members, remove_from_contest, remove_member, reset_password, update, update_member,
};
pub use service::TeamService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/teams", axum::routing::get(list).post(create))
        .route("/api/teams/{team_id}", axum::routing::get(get).patch(update).delete(delete))
        .route("/api/teams/batch", axum::routing::post(batch_import))
        .route("/api/teams/{team_id}/members", axum::routing::get(list_members).post(add_member))
        .route(
            "/api/teams/{team_id}/members/{member_id}",
            axum::routing::patch(update_member).delete(remove_member),
        )
        .route("/api/teams/{team_id}/account/reset-password", axum::routing::post(reset_password))
        .route(
            "/api/contests/{contest_id}/teams",
            axum::routing::get(list_contest_teams).post(assign_to_contest),
        )
        .route(
            "/api/contests/{contest_id}/teams/{team_id}",
            axum::routing::delete(remove_from_contest),
        )
}
