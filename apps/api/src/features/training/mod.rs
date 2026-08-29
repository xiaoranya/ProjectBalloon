mod bank;
mod model;
mod practice;
mod sets;

pub use bank::*;
pub use model::*;
pub use practice::*;
pub use sets::*;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/public/problem-bank", get(list_bank))
        .route("/api/public/problem-bank/{slug}", get(get_bank))
        .route(
            "/api/admin/problems/{problem_id}/publication",
            get(get_publication).put(update_publication),
        )
        .route("/api/admin/training/sets", post(create_set))
        .route("/api/admin/training/sets/{set_id}", put(update_set))
        .route(
            "/api/admin/practice/settings",
            get(get_practice_settings).put(update_practice_settings),
        )
        .route("/api/training/sets", get(list_sets))
        .route("/api/training/sets/{set_id}", get(get_set))
        .route("/api/training/sets/{set_id}/enroll", post(enroll))
        .route("/api/training/enrollments/{enrollment_id}/progress", put(progress))
        .route("/api/practice/favorites", get(list_favorites))
        .route("/api/practice/problems/{problem_id}/favorite", put(set_favorite))
        .route("/api/practice/problems/{problem_id}/editorial", get(get_editorial))
        .route(
            "/api/admin/problems/{problem_id}/editorials/{lang_code}",
            get(get_admin_editorial).put(upsert_editorial),
        )
}

#[cfg(test)]
mod tests;

use axum::routing::{get, post, put};
