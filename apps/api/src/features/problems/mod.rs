pub(crate) mod handlers;
mod markdown;
mod model;
mod service;
mod testdata_archive;

pub use handlers::{
    activate_testdata_version, create, delete, delete_attachment, delete_statement,
    download_attachment, download_testdata, download_testdata_version, get, list, list_attachments,
    list_statements, list_testdata_versions, update, upload_attachment, upload_interactor,
    upload_testdata, upsert_statement,
};
pub(crate) use markdown::render_safe as render_safe_statement;
pub use service::ProblemService;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/problems", axum::routing::get(list).post(create))
        .route("/api/problems/{problem_id}", axum::routing::get(get).patch(update).delete(delete))
        .route(
            "/api/problems/{problem_id}/statements/{lang_code}",
            axum::routing::put(upsert_statement).delete(delete_statement),
        )
        .route("/api/problems/{problem_id}/statements", axum::routing::get(list_statements))
        .route(
            "/api/problems/{problem_id}/testdata",
            axum::routing::get(download_testdata)
                .post(upload_testdata)
                .layer(DefaultBodyLimit::max(258 * 1024 * 1024)),
        )
        .route(
            "/api/problems/{problem_id}/interactor",
            axum::routing::post(upload_interactor).layer(DefaultBodyLimit::max(20 * 1024 * 1024)),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions",
            axum::routing::get(list_testdata_versions),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions/{version}",
            axum::routing::get(download_testdata_version),
        )
        .route(
            "/api/problems/{problem_id}/testdata/versions/{version}/activate",
            axum::routing::post(activate_testdata_version),
        )
        .route(
            "/api/problems/{problem_id}/attachments",
            axum::routing::get(list_attachments)
                .post(upload_attachment)
                .layer(DefaultBodyLimit::max(22 * 1024 * 1024)),
        )
        .route(
            "/api/problems/{problem_id}/attachments/{attachment_id}",
            axum::routing::get(download_attachment).delete(delete_attachment),
        )
}

use axum::extract::DefaultBodyLimit;
