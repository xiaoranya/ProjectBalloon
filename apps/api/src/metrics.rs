use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use sqlx::{FromRow, PgPool};
use subtle::ConstantTimeEq;

use crate::{error::AppError, state::AppState};
use axum::routing::get;

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
    practice_submissions_today: i64,
    practice_judging: i64,
    judging_stuck: i64,
}

#[utoipa::path(
    get,
    path = "/metrics",
    operation_id = "getPrometheusMetrics",
    tag = "observability",
    responses(
        (status = 200, description = "Prometheus text exposition", body = String, content_type = "text/plain"),
        (status = 401, description = "A metrics token is configured and the bearer token is missing or wrong", body = crate::error::ApiErrorBody)
    ),
    security(())
)]
pub(crate) async fn prometheus(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    if let Some(expected) = state.metrics_token() {
        authorize_metrics_request(&headers, expected)?;
    }
    let snapshot = collect_snapshot(state.database())
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

/// Enforces the optional `/metrics` bearer token. Queue depths, worker
/// capacity, and submission volumes are operational data that must not be
/// world-readable wherever a token is configured.
fn authorize_metrics_request(
    headers: &axum::http::HeaderMap,
    expected: &str,
) -> Result<(), AppError> {
    const BEARER_PREFIX: &str = "Bearer ";
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
        .unwrap_or("");
    // Constant-time comparison so response timing cannot probe the token.
    if bool::from(expected.as_bytes().ct_eq(provided.as_bytes())) {
        Ok(())
    } else {
        Err(AppError::unauthorized(
            "METRICS_UNAUTHORIZED",
            "Metrics endpoint requires a valid bearer token",
        ))
    }
}

async fn collect_snapshot(database: &PgPool) -> Result<MetricsSnapshot, sqlx::Error> {
    sqlx::query_as::<_, MetricsSnapshot>(
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
            ,(SELECT count(*) FROM submissions WHERE submission_scope='PRACTICE' AND submitted_at >= date_trunc('day', now())) AS practice_submissions_today
            ,(SELECT count(*) FROM submissions WHERE submission_scope='PRACTICE' AND status IN ('PENDING','JUDGING')) AS practice_judging
            ,(SELECT count(*) FROM submissions
              WHERE status IN ('PENDING','JUDGING')
                AND submitted_at < now() - interval '10 minutes') AS judging_stuck
        "#,
    )
    .fetch_one(database)
    .await
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
        (
            "project_balloon_practice_submissions_today",
            "Practice submissions created since the start of the current day",
            value.practice_submissions_today,
        ),
        (
            "project_balloon_practice_judging",
            "Practice submissions currently waiting for or receiving a judgement",
            value.practice_judging,
        ),
        (
            "project_balloon_submissions_stuck_judging",
            "Submissions still PENDING or JUDGING 10 minutes after submission",
            value.judging_stuck,
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

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new().route("/metrics", get(prometheus))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, header};
    use sqlx::PgPool;

    use crate::metrics::{MetricsSnapshot, authorize_metrics_request, collect_snapshot, render};

    fn bearer_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, format!("Bearer {token}").parse().expect("header"));
        headers
    }

    #[test]
    fn metrics_token_authorizes_only_the_configured_bearer() {
        assert!(authorize_metrics_request(&bearer_headers("secret-token"), "secret-token").is_ok());
    }

    #[test]
    fn metrics_token_rejects_missing_wrong_and_malformed_credentials() {
        let empty = HeaderMap::new();
        assert!(authorize_metrics_request(&empty, "secret-token").is_err());
        assert!(authorize_metrics_request(&bearer_headers("wrong"), "secret-token").is_err());
        assert!(authorize_metrics_request(&bearer_headers(""), "secret-token").is_err());
        let mut basic_only = HeaderMap::new();
        basic_only.insert(header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().expect("header"));
        assert!(authorize_metrics_request(&basic_only, "secret-token").is_err());
    }

    #[test]
    fn metrics_token_rejects_a_token_that_merely_prefixes_the_expected_one() {
        assert!(authorize_metrics_request(&bearer_headers("secret-toke"), "secret-token").is_err());
    }

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
            practice_submissions_today: 12,
            practice_judging: 2,
            judging_stuck: 1,
        });
        assert!(output.contains("# TYPE project_balloon_judge_worker_capacity gauge"));
        assert!(output.contains("project_balloon_judge_worker_capacity 30\n"));
        assert!(output.contains("project_balloon_object_storage_missing_references 2\n"));
        assert!(output.contains("project_balloon_practice_submissions_today 12\n"));
        assert!(output.contains("project_balloon_submissions_stuck_judging 1\n"));
        assert!(output.ends_with('\n'));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn worker_capacity_counts_only_heartbeat_fresh_workers(pool: PgPool) {
        sqlx::query(
            r#"
            INSERT INTO judge_workers
                (worker_id, instance_id, started_at, last_seen_at, capacity, active_tasks,
                 languages, runtime_versions, last_message_id)
            VALUES ('stale-worker', $1, now() - interval '1 hour', now() - interval '2 minutes',
                    4, 2, '[]'::jsonb, '{}'::jsonb, $2)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(uuid::Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("insert stale worker");

        let stale = collect_snapshot(&pool).await.expect("snapshot with only stale worker");
        assert_eq!(stale.worker_capacity, 0, "stale heartbeats must not contribute capacity");
        assert_eq!(stale.worker_active, 0);

        sqlx::query(
            r#"
            INSERT INTO judge_workers
                (worker_id, instance_id, started_at, last_seen_at, capacity, active_tasks,
                 languages, runtime_versions, last_message_id)
            VALUES ('fresh-worker', $1, now(), now(), 3, 1, '[]'::jsonb, '{}'::jsonb, $2)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(uuid::Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("insert fresh worker");

        let fresh = collect_snapshot(&pool).await.expect("snapshot with a fresh worker");
        assert_eq!(fresh.worker_capacity, 3, "COALESCE must expose fresh capacity");
        assert_eq!(fresh.worker_active, 1);
    }
}
