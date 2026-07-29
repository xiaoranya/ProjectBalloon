use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use sqlx::FromRow;

use crate::{error::AppError, state::AppState};

#[derive(Debug, FromRow)]
struct MetricsSnapshot {
    realtime_pending: i64,
    realtime_failed: i64,
    judge_pending: i64,
    judge_failed: i64,
    cleanup_pending: i64,
    cleanup_failed: i64,
    storage_missing_references: i64,
    exports_queued: i64,
    exports_processing: i64,
    exports_failed: i64,
    worker_capacity: i64,
    worker_active: i64,
}

#[utoipa::path(
    get,
    path = "/metrics",
    operation_id = "getPrometheusMetrics",
    tag = "observability",
    responses((status = 200, description = "Prometheus text exposition", body = String, content_type = "text/plain")),
    security(())
)]
pub(crate) async fn prometheus(State(state): State<AppState>) -> Result<Response, AppError> {
    let snapshot = sqlx::query_as::<_, MetricsSnapshot>(
        r#"
        SELECT
            (SELECT count(*) FROM realtime_outbox WHERE status IN ('PENDING', 'PUBLISHING')) AS realtime_pending,
            (SELECT count(*) FROM realtime_outbox WHERE status = 'FAILED') AS realtime_failed,
            (SELECT count(*) FROM submission_outbox WHERE status IN ('PENDING', 'PUBLISHING')) AS judge_pending,
            (SELECT count(*) FROM submission_outbox WHERE status = 'FAILED') AS judge_failed,
            (SELECT count(*) FROM object_storage_cleanup_tasks WHERE status IN ('PENDING', 'PROCESSING')) AS cleanup_pending,
            (SELECT count(*) FROM object_storage_cleanup_tasks WHERE status = 'FAILED') AS cleanup_failed,
            (SELECT count(*) FROM object_storage_integrity_findings WHERE resolved_at IS NULL) AS storage_missing_references,
            (SELECT count(*) FROM submission_export_tasks WHERE status = 'QUEUED') AS exports_queued,
            (SELECT count(*) FROM submission_export_tasks WHERE status = 'PROCESSING') AS exports_processing,
            (SELECT count(*) FROM submission_export_tasks WHERE status = 'FAILED') AS exports_failed,
            (SELECT COALESCE(sum(capacity), 0) FROM judge_workers WHERE last_seen_at >= now() - interval '30 seconds') AS worker_capacity,
            (SELECT COALESCE(sum(active_tasks), 0) FROM judge_workers WHERE last_seen_at >= now() - interval '30 seconds') AS worker_active
        "#,
    )
    .fetch_one(state.database())
    .await
    .map_err(|error| AppError::internal("collect Prometheus metrics", error))?;
    let body = render(&snapshot);
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        body,
    )
        .into_response())
}

fn render(value: &MetricsSnapshot) -> String {
    let gauges = [
        (
            "project_balloon_realtime_outbox_pending",
            "Realtime outbox rows awaiting confirmed delivery",
            value.realtime_pending,
        ),
        (
            "project_balloon_realtime_outbox_failed",
            "Realtime outbox rows waiting for retry",
            value.realtime_failed,
        ),
        (
            "project_balloon_judge_outbox_pending",
            "Judge tasks awaiting confirmed publication",
            value.judge_pending,
        ),
        (
            "project_balloon_judge_outbox_failed",
            "Judge tasks waiting for retry",
            value.judge_failed,
        ),
        (
            "project_balloon_object_cleanup_pending",
            "Object cleanup rows awaiting completion",
            value.cleanup_pending,
        ),
        (
            "project_balloon_object_cleanup_failed",
            "Object cleanup rows waiting for retry",
            value.cleanup_failed,
        ),
        (
            "project_balloon_object_storage_missing_references",
            "Database object references missing from object storage",
            value.storage_missing_references,
        ),
        ("project_balloon_export_tasks_queued", "Submission exports queued", value.exports_queued),
        (
            "project_balloon_export_tasks_processing",
            "Submission exports currently processing",
            value.exports_processing,
        ),
        (
            "project_balloon_export_tasks_failed",
            "Submission exports waiting for retry",
            value.exports_failed,
        ),
        (
            "project_balloon_judge_worker_capacity",
            "Online judge worker slot capacity",
            value.worker_capacity,
        ),
        (
            "project_balloon_judge_worker_active_tasks",
            "Active tasks reported by online judge workers",
            value.worker_active,
        ),
    ];
    let mut output = String::with_capacity(gauges.len() * 180);
    for (name, help, metric) in gauges {
        output.push_str("# HELP ");
        output.push_str(name);
        output.push(' ');
        output.push_str(help);
        output.push('\n');
        output.push_str("# TYPE ");
        output.push_str(name);
        output.push_str(" gauge\n");
        output.push_str(name);
        output.push(' ');
        output.push_str(&metric.to_string());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{MetricsSnapshot, render};

    #[test]
    fn prometheus_output_has_help_type_and_values() {
        let output = render(&MetricsSnapshot {
            realtime_pending: 2,
            realtime_failed: 0,
            judge_pending: 3,
            judge_failed: 1,
            cleanup_pending: 4,
            cleanup_failed: 0,
            storage_missing_references: 2,
            exports_queued: 5,
            exports_processing: 1,
            exports_failed: 0,
            worker_capacity: 30,
            worker_active: 7,
        });
        assert!(output.contains("# TYPE project_balloon_judge_worker_capacity gauge"));
        assert!(output.contains("project_balloon_judge_worker_capacity 30\n"));
        assert!(output.contains("project_balloon_object_storage_missing_references 2\n"));
        assert!(output.ends_with('\n'));
    }
}
