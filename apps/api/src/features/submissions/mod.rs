mod bulk_rejudge;
mod export_tasks;
mod exports;
pub(crate) mod handlers;
mod model;
mod practice;
mod query;
mod service;

pub use bulk_rejudge::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgePreviewResponse, BatchRejudgeRunner,
    BatchRejudgeService, BatchRejudgeTaskResponse,
};
pub use export_tasks::{
    ClaimedExportTask, CreateExportTaskRequest, ExportTaskKind, ExportTaskResponse,
    ExportTaskRunner, ExportTaskRunnerConfig,
};
pub use handlers::{
    backfill_similarity, create_batch_rejudge, create_export_task, detail_admin, detail_own,
    download_export_task, export_metadata_csv, export_sources_zip, get_batch_rejudge,
    get_export_task, judge_queue_status, list_admin, list_batch_rejudge, list_own, list_practice,
    list_similarity, list_similarity_pairs, pause_batch_rejudge, practice_detail,
    practice_progress, preview_batch_rejudge, rejudge, resume_batch_rejudge, submit,
    submit_practice,
};
pub use service::SubmissionService;

pub(crate) use model::SubmissionStatus;

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route(
            "/api/practice/submissions",
            get(list_practice).post(submit_practice).layer(DefaultBodyLimit::max(70 * 1024)),
        )
        .route("/api/practice/submissions/{submission_id}", get(practice_detail))
        .route("/api/practice/progress", get(practice_progress))
        .route(
            "/api/contests/{contest_id}/submissions",
            get(list_own).post(submit).layer(DefaultBodyLimit::max(70 * 1024)),
        )
        .route("/api/contests/{contest_id}/submissions/{submission_id}", get(detail_own))
        .route("/api/admin/contests/{contest_id}/submissions", get(list_admin))
        .route("/api/admin/contests/{contest_id}/submission-similarity", get(list_similarity))
        .route(
            "/api/admin/contests/{contest_id}/submission-similarity/pairs",
            get(list_similarity_pairs),
        )
        .route(
            "/api/admin/contests/{contest_id}/submission-similarity/backfill",
            post(backfill_similarity),
        )
        .route("/api/admin/contests/{contest_id}/judge-queue/status", get(judge_queue_status))
        .route("/api/admin/contests/{contest_id}/submissions/{submission_id}", get(detail_admin))
        .route(
            "/api/admin/contests/{contest_id}/submissions/{submission_id}/rejudge",
            post(rejudge),
        )
        .route("/api/admin/contests/{contest_id}/exports/submissions.csv", get(export_metadata_csv))
        .route(
            "/api/admin/contests/{contest_id}/exports/submission-sources.zip",
            get(export_sources_zip),
        )
        .route("/api/admin/contests/{contest_id}/exports/tasks", post(create_export_task))
        .route("/api/admin/contests/{contest_id}/exports/tasks/{task_id}", get(get_export_task))
        .route(
            "/api/admin/contests/{contest_id}/exports/tasks/{task_id}/download",
            get(download_export_task),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/preview",
            post(preview_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks",
            get(list_batch_rejudge).post(create_batch_rejudge),
        )
        .route("/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}", get(get_batch_rejudge))
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/pause",
            post(pause_batch_rejudge),
        )
        .route(
            "/api/admin/contests/{contest_id}/rejudge-tasks/{task_id}/resume",
            post(resume_batch_rejudge),
        )
}

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
