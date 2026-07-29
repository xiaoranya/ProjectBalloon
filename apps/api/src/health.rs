use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::time::timeout;
use tracing::warn;
use utoipa::ToSchema;

use crate::{SERVICE_NAME, state::AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
enum HealthStatus {
    Up,
    Down,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthResponse {
    status: HealthStatus,
    service: &'static str,
    #[serde(with = "time::serde::rfc3339")]
    time: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    realtime_outbox: Option<RealtimeOutboxHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    judge_dispatch: Option<JudgeDispatchHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    object_storage: Option<DependencyHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    object_cleanup: Option<ObjectCleanupHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cups: Option<DependencyHealth>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RealtimeOutboxHealth {
    pending: i64,
    failed: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    redis_connected: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ObjectCleanupHealth {
    pending: i64,
    failed: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct JudgeDispatchHealth {
    pending: i64,
    failed: i64,
    workers: WorkerFleetHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    rabbitmq: Option<RabbitMqHealth>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct WorkerFleetHealth {
    online: i64,
    stale: i64,
    capacity: i64,
    active_tasks: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct RabbitMqHealth {
    status: HealthStatus,
    queued_tasks: u32,
    queued_results: u32,
    dead_tasks: u32,
}

#[derive(Debug, Serialize, ToSchema)]
struct DependencyHealth {
    status: HealthStatus,
}

#[utoipa::path(
    get,
    path = "/livez",
    operation_id = "getLiveness",
    tag = "health",
    responses((status = 200, description = "Process is alive", body = HealthResponse)),
    security(())
)]
pub(crate) async fn liveness() -> Json<HealthResponse> {
    Json(response(HealthStatus::Up, None, None, None, None, None))
}

#[utoipa::path(
    get,
    path = "/api/health",
    operation_id = "getReadiness",
    tag = "health",
    responses(
        (status = 200, description = "Service and configured dependencies are ready", body = HealthResponse),
        (status = 503, description = "A required dependency is unavailable", body = HealthResponse)
    ),
    security(())
)]
pub(crate) async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let probe = timeout(
        state.readiness_timeout(),
        sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
            r#"
            SELECT
                (SELECT count(*) FROM realtime_outbox
                 WHERE status IN ('PENDING', 'PUBLISHING')),
                (SELECT count(*) FROM realtime_outbox WHERE status = 'FAILED'),
                (SELECT count(*) FROM submission_outbox
                 WHERE status IN ('PENDING', 'PUBLISHING')),
                (SELECT count(*) FROM submission_outbox WHERE status = 'FAILED'),
                (SELECT count(*) FROM judge_workers
                 WHERE last_seen_at >= now() - interval '15 seconds'),
                (SELECT count(*) FROM judge_workers
                 WHERE last_seen_at < now() - interval '15 seconds'),
                (SELECT coalesce(sum(capacity), 0)::bigint FROM judge_workers
                 WHERE last_seen_at >= now() - interval '15 seconds'),
                (SELECT coalesce(sum(active_tasks), 0)::bigint FROM judge_workers
                 WHERE last_seen_at >= now() - interval '15 seconds'),
                (SELECT count(*) FROM object_storage_cleanup_tasks
                 WHERE status IN ('PENDING', 'PROCESSING')),
                (SELECT count(*) FROM object_storage_cleanup_tasks WHERE status = 'FAILED')
            "#,
        )
        .fetch_one(state.database()),
    )
    .await;

    match probe {
        Ok(Ok((
            pending,
            failed,
            judge_pending,
            judge_failed,
            online_workers,
            stale_workers,
            worker_capacity,
            worker_active_tasks,
            cleanup_pending,
            cleanup_failed,
        ))) => {
            let redis_connected = state.realtime().redis_status();
            let object_storage = match state.object_storage() {
                Some(storage) => match timeout(state.readiness_timeout(), storage.check()).await {
                    Ok(Ok(())) => Some(DependencyHealth { status: HealthStatus::Up }),
                    Ok(Err(error)) => {
                        warn!(error = %error, "object storage readiness probe failed");
                        Some(DependencyHealth { status: HealthStatus::Down })
                    }
                    Err(_) => {
                        warn!("object storage readiness probe timed out");
                        Some(DependencyHealth { status: HealthStatus::Down })
                    }
                },
                None => None,
            };
            let rabbitmq = match state.judge_publisher() {
                Some(publisher) => {
                    match timeout(state.readiness_timeout(), publisher.probe()).await {
                        Ok(Ok(probe)) => Some(RabbitMqHealth {
                            status: HealthStatus::Up,
                            queued_tasks: probe.queued_tasks,
                            queued_results: probe.queued_results,
                            dead_tasks: probe.dead_tasks,
                        }),
                        Ok(Err(error)) => {
                            warn!(%error, "RabbitMQ readiness probe failed");
                            Some(RabbitMqHealth {
                                status: HealthStatus::Down,
                                queued_tasks: 0,
                                queued_results: 0,
                                dead_tasks: 0,
                            })
                        }
                        Err(_) => {
                            warn!("RabbitMQ readiness probe timed out");
                            Some(RabbitMqHealth {
                                status: HealthStatus::Down,
                                queued_tasks: 0,
                                queued_results: 0,
                                dead_tasks: 0,
                            })
                        }
                    }
                }
                None => None,
            };
            let cups = match state.cups_gateway() {
                Some(gateway) => match timeout(state.readiness_timeout(), gateway.probe()).await {
                    Ok(Ok(())) => Some(DependencyHealth { status: HealthStatus::Up }),
                    Ok(Err(error)) => {
                        warn!(%error, "CUPS readiness probe failed");
                        Some(DependencyHealth { status: HealthStatus::Down })
                    }
                    Err(_) => {
                        warn!("CUPS readiness probe timed out");
                        Some(DependencyHealth { status: HealthStatus::Down })
                    }
                },
                None => None,
            };
            let status = if redis_connected == Some(false)
                || object_storage.as_ref().is_some_and(|health| health.status == HealthStatus::Down)
                || rabbitmq.as_ref().is_some_and(|health| health.status == HealthStatus::Down)
                || cups.as_ref().is_some_and(|health| health.status == HealthStatus::Down)
            {
                HealthStatus::Down
            } else {
                HealthStatus::Up
            };
            let status_code = if status == HealthStatus::Up {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            (
                status_code,
                Json(response(
                    status,
                    Some(RealtimeOutboxHealth { pending, failed, redis_connected }),
                    object_storage,
                    Some(ObjectCleanupHealth { pending: cleanup_pending, failed: cleanup_failed }),
                    Some(JudgeDispatchHealth {
                        pending: judge_pending,
                        failed: judge_failed,
                        workers: WorkerFleetHealth {
                            online: online_workers,
                            stale: stale_workers,
                            capacity: worker_capacity,
                            active_tasks: worker_active_tasks,
                        },
                        rabbitmq,
                    }),
                    cups,
                )),
            )
        }
        Ok(Err(error)) => {
            warn!(error = %error, "database readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(response(HealthStatus::Down, None, None, None, None, None)),
            )
        }
        Err(_) => {
            warn!("database readiness probe timed out");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(response(HealthStatus::Down, None, None, None, None, None)),
            )
        }
    }
}

fn response(
    status: HealthStatus,
    realtime_outbox: Option<RealtimeOutboxHealth>,
    object_storage: Option<DependencyHealth>,
    object_cleanup: Option<ObjectCleanupHealth>,
    judge_dispatch: Option<JudgeDispatchHealth>,
    cups: Option<DependencyHealth>,
) -> HealthResponse {
    HealthResponse {
        status,
        service: SERVICE_NAME,
        time: OffsetDateTime::now_utc(),
        realtime_outbox,
        object_storage,
        object_cleanup,
        judge_dispatch,
        cups,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use crate::{SERVICE_NAME, router, state::AppState};

    #[tokio::test]
    async fn liveness_does_not_require_database() {
        let response = test_router()
            .oneshot(Request::get("/livez").body(Body::empty()).expect("valid request"))
            .await
            .expect("router must serve the request");

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["status"], "up");
        assert_eq!(body["service"], SERVICE_NAME);
    }

    #[tokio::test]
    async fn readiness_is_down_when_database_is_unavailable() {
        let response = test_router()
            .oneshot(Request::get("/api/health").body(Body::empty()).expect("valid request"))
            .await
            .expect("router must serve the request");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_body(response).await;
        assert_eq!(body["status"], "down");
        assert_eq!(body["service"], SERVICE_NAME);
    }

    fn test_router() -> axum::Router {
        let pool = PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(5))
            .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/unavailable")
            .expect("test database URL must parse");
        router(
            AppState::new(
                pool,
                Duration::from_millis(10),
                Duration::from_secs(60),
                false,
                b"test-csrf-secret-with-at-least-32-bytes",
                16,
                false,
            ),
            vec!["127.0.0.1/32".parse().expect("CIDR")],
        )
    }

    async fn json_body(response: axum::response::Response) -> Value {
        let bytes = response.into_body().collect().await.expect("valid response body").to_bytes();
        serde_json::from_slice(&bytes).expect("valid JSON response")
    }
}
