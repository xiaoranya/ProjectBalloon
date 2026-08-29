mod cache;
pub(crate) mod handlers;
mod helpers;
mod model;
mod projection;
mod service;
/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/contests/{contest_id}/scoreboard", get(public))
        .route("/api/contests/{contest_id}/scoreboard.csv", get(public_csv))
        .route("/api/admin/contests/{contest_id}/scoreboard", get(admin))
        .route("/api/admin/contests/{contest_id}/scoreboard.csv", get(admin_csv))
        .route("/api/admin/contests/{contest_id}/scoreboard/snapshots", post(create_snapshot))
        .route("/api/admin/contests/{contest_id}/scoreboard/snapshots/latest", get(latest_snapshot))
}

#[cfg(test)]
mod tests;

pub use cache::ScoreboardCache;
pub use handlers::{admin, admin_csv, create_snapshot, latest_snapshot, public, public_csv};
pub use model::{
    ScoreboardCell, ScoreboardProblem, ScoreboardQuery, ScoreboardResponse, ScoreboardRow,
};
pub(crate) use projection::rebuild_cell;
pub use service::ScoreboardService;

use axum::routing::{get, post};
