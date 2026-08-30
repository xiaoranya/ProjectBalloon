use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{Json, Router, extract::State, routing::get};
use serde_json::json;
use tokio::sync::watch;
use tracing::info;

/// Shared liveness/readiness bookkeeping for the worker's health endpoints.
///
/// `/livez` answers 200 unconditionally: reaching it proves the process is
/// alive. `/readyz` answers 200 only once at least one AMQP consume session
/// has been established and the most recent session failure is older than the
/// configured window — a worker whose broker session keeps flapping must not
/// receive docker healthcheck blessings.
#[derive(Clone, Default)]
pub struct HealthState {
    inner: Arc<Mutex<HealthInner>>,
    session_error_window: Duration,
}

#[derive(Debug, Default)]
struct HealthInner {
    session_started_at: Option<Instant>,
    last_session_error: Option<(Instant, String)>,
}

impl HealthState {
    #[must_use]
    pub fn new(session_error_window: Duration) -> Self {
        Self { inner: Arc::new(Mutex::new(HealthInner::default())), session_error_window }
    }

    /// Marks that a consume session was fully established (connection,
    /// topology, and consumer are live). A reconnection after a failure also
    /// clears that failure: the worker is demonstrably healthy again.
    pub fn record_session_started(&self) {
        let mut inner = self.inner.lock().expect("health state lock");
        inner.session_started_at = Some(Instant::now());
        inner.last_session_error = None;
    }

    /// Records why the latest consume session failed, with its timestamp.
    pub fn record_session_failed(&self, reason: String) {
        self.inner.lock().expect("health state lock").last_session_error =
            Some((Instant::now(), reason));
    }

    pub fn ready(&self) -> Result<(), String> {
        let inner = self.inner.lock().expect("health state lock");
        if inner.session_started_at.is_none() {
            return Err("no consume session has been established yet".to_owned());
        }
        if let Some((failed_at, reason)) = &inner.last_session_error {
            let elapsed = failed_at.elapsed();
            if elapsed < self.session_error_window {
                return Err(format!("last consume session failed {elapsed:?} ago: {reason}"));
            }
        }
        Ok(())
    }
}

/// Assembles the health router: `GET /livez` (always 200) and `GET /readyz`
/// (200 when ready, 503 with a JSON reason otherwise).
pub fn health_router(state: HealthState) -> Router {
    Router::new().route("/livez", get(livez)).route("/readyz", get(readyz)).with_state(state)
}

async fn livez() -> &'static str {
    "ok"
}

async fn readyz(
    State(state): State<HealthState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    match state.ready() {
        Ok(()) => Ok(Json(json!({ "ready": true }))),
        Err(reason) => Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "ready": false, "reason": reason })),
        )),
    }
}

/// Serves the health endpoints until the shutdown watch flips. Binds
/// loopback only: the docker healthcheck probes from inside the container.
pub async fn serve_health(
    addr: SocketAddr,
    state: HealthState,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "judge worker health endpoint listening");
    axum::serve(listener, health_router(state))
        .with_graceful_shutdown(async move {
            loop {
                if *shutdown.borrow() {
                    return;
                }
                if shutdown.changed().await.is_err() {
                    return;
                }
            }
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{body::Body, http::Request, http::StatusCode};
    use tower::ServiceExt;

    use super::{HealthState, health_router};

    #[tokio::test]
    async fn livez_is_always_ready_and_readyz_tracks_sessions() {
        let state = HealthState::new(Duration::from_secs(60));
        let app = health_router(state.clone());

        let livez = app
            .clone()
            .oneshot(Request::get("/livez").body(Body::empty()).expect("request"))
            .await
            .expect("livez response");
        assert_eq!(livez.status(), StatusCode::OK);

        let unready = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).expect("request"))
            .await
            .expect("readyz response");
        assert_eq!(unready.status(), StatusCode::SERVICE_UNAVAILABLE);

        state.record_session_failed("connection refused".to_owned());
        state.record_session_started();
        let recovered = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).expect("request"))
            .await
            .expect("readyz response");
        assert_eq!(recovered.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn recent_session_failure_flips_readyz_to_unavailable() {
        let state = HealthState::new(Duration::from_secs(60));
        let app = health_router(state.clone());
        state.record_session_started();
        state.record_session_failed("broker went away".to_owned());

        let response = app
            .oneshot(Request::get("/readyz").body(Body::empty()).expect("request"))
            .await
            .expect("readyz response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn session_failure_ages_out_after_the_window() {
        let state = HealthState::new(Duration::from_millis(1));
        let app = health_router(state.clone());
        state.record_session_started();
        state.record_session_failed("broker went away".to_owned());
        tokio::time::sleep(Duration::from_millis(5)).await;

        let response = app
            .oneshot(Request::get("/readyz").body(Body::empty()).expect("request"))
            .await
            .expect("readyz response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}
