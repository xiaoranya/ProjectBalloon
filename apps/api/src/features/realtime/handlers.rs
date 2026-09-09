use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, State},
    http::{HeaderValue, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::{StreamExt, stream};
use project_balloon_contracts::{RealtimeEvent, RealtimeScope};

use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{AuthContext, OptionalAuthContext, model::UserType},
    state::AppState,
};

use crate::features::realtime::hub::RealtimeEnvelope;

/// Resolves the Last-Event-ID resume window from the Redis outbox replay
/// index. A missing Redis handle (test harnesses) or a Redis failure degrades
/// to no replay: live events still stream, and the client's poll-based full
/// refresh covers the gap.
async fn replay(
    state: &AppState,
    contest_id: i64,
    scope: RealtimeScope,
    team_id: Option<i64>,
    last_event_id: Option<Uuid>,
) -> Vec<RealtimeEvent> {
    let Some(last_event_id) = last_event_id else { return Vec::new() };
    let Some(outbox) = state.realtime_outbox() else { return Vec::new() };
    match outbox.load_replay(contest_id, scope, team_id, last_event_id).await {
        Ok(replay) => replay,
        Err(error) => {
            tracing::warn!(?error, %contest_id, "realtime replay lookup failed; continuing without replay");
            Vec::new()
        }
    }
}

#[utoipa::path(get, path = "/api/public/events/contests/{contest_id}", operation_id = "subscribePublicContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent public contest events", content_type = "text/event-stream", body = String), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn subscribe_public(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    last_event_id: LastEventId,
) -> Result<Response, AppError> {
    state.contests().get(contest_id, context.user()).await?;
    let replay = replay(&state, contest_id, RealtimeScope::Public, None, last_event_id.0).await;
    Ok(stream_response(
        state.realtime().subscribe(),
        state.shutdown_receiver(),
        contest_id,
        RealtimeScope::Public,
        None,
        replay,
    ))
}

#[utoipa::path(get, path = "/api/events/contests/{contest_id}", operation_id = "subscribeStaffContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent staff contest events", content_type = "text/event-stream", body = String), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn subscribe_staff(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    last_event_id: LastEventId,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    if !context.user().user_type.is_staff() {
        return Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"));
    }
    state.contests().get(contest_id, Some(context.user())).await?;
    if context.user().has_permission(crate::features::auth::permissions::CONTEST_MANAGE)
        && !context.user().is_super_admin()
    {
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE contest_id=$1 AND user_id=$2)",
        )
        .bind(contest_id)
        .bind(context.user().id)
        .fetch_one(state.database())
        .await
        .map_err(|error| AppError::internal("check staff contest scope", error))?;
        if !assigned {
            return Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest not found"));
        }
    }
    let replay = replay(&state, contest_id, RealtimeScope::Staff, None, last_event_id.0).await;
    Ok(stream_response(
        state.realtime().subscribe(),
        state.shutdown_receiver(),
        contest_id,
        RealtimeScope::Staff,
        None,
        replay,
    ))
}

#[utoipa::path(get, path = "/api/team/events/contests/{contest_id}", operation_id = "subscribeTeamContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent team contest events", content_type = "text/event-stream", body = String), (status = 401, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn subscribe_team(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    last_event_id: LastEventId,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    if context.user().user_type != UserType::Team {
        return Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest not found"));
    }
    let team_id = state.contests().require_team_id(contest_id, context.user().id).await?;
    let replay =
        replay(&state, contest_id, RealtimeScope::Team, Some(team_id), last_event_id.0).await;
    Ok(stream_response(
        state.realtime().subscribe(),
        state.shutdown_receiver(),
        contest_id,
        RealtimeScope::Team,
        Some(team_id),
        replay,
    ))
}

/// Resolved Last-Event-ID resume position: `None` means "no replay".
pub struct LastEventId(pub Option<Uuid>);

impl<S> axum::extract::FromRequestParts<S> for LastEventId
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if let Some(value) =
            parts.headers.get("last-event-id").and_then(|value| value.to_str().ok())
        {
            return Ok(Self(Uuid::parse_str(value).ok()));
        }
        // The browser only attaches the header on its own automatic reconnect;
        // our client rebuilds the EventSource manually, so it passes the id as
        // a query parameter instead.
        let Some(query) = &parts.uri.query() else { return Ok(Self(None)) };
        for pair in query.split('&') {
            let Some((name, value)) = pair.split_once('=') else { continue };
            if name == "lastEventId" {
                return Ok(Self(Uuid::parse_str(value).ok()));
            }
        }
        Ok(Self(None))
    }
}

/// Frames each subscription into an SSE response. The stream ends when the
/// hub's sender drops *or* the process shutdown watch fires; the latter is the
/// only way to end the stream during graceful shutdown, because the hub's
/// sender lives in the `AppState` the server future is itself awaiting.
fn stream_response(
    receiver: broadcast::Receiver<RealtimeEnvelope>,
    shutdown: watch::Receiver<bool>,
    contest_id: i64,
    scope: RealtimeScope,
    team_id: Option<i64>,
    replay: Vec<RealtimeEvent>,
) -> Response {
    let replay_frames = stream::iter(replay).map(|event| event_frame(&event));
    let connected =
        stream::once(async move { event_frame(&RealtimeEvent::connected(contest_id, scope)) });
    let messages = stream::unfold(
        (receiver, shutdown),
        move |(mut receiver, mut shutdown)| async move {
            loop {
                if *shutdown.borrow_and_update() {
                    return None;
                }
                tokio::select! {
                    changed = shutdown.changed() => {
                        // A dropped sender means no shutdown can ever be signaled
                        // through this clone; fall back to the hub's own close.
                        if changed.is_ok() && *shutdown.borrow_and_update() {
                            return None;
                        }
                    }
                    received = receiver.recv() => {
                        match received {
                            Ok(envelope)
                                if envelope.event.contest_id == contest_id
                                    && envelope.event.scope == scope
                                    && envelope.team_id == team_id =>
                            {
                                return Some((event_frame(&envelope.event), (receiver, shutdown)));
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                tracing::warn!(
                                    %contest_id,
                                    %skipped,
                                    scope = %scope.as_str(),
                                    team_id,
                                    "realtime subscriber lagged; dropped events will not be replayed"
                                );
                            }
                            Err(broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                }
            }
        },
    );
    let stream = replay_frames.chain(connected).chain(messages);
    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("heartbeat"))
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-transform"));
    response.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

fn event_frame(event: &RealtimeEvent) -> Result<Event, Infallible> {
    let frame = Event::default()
        .id(event.id.to_string())
        .event("message")
        .retry(Duration::from_secs(3))
        .json_data(event)
        .unwrap_or_else(|error| {
            tracing::error!(%error, "failed to serialize realtime event");
            Event::default().event("error").data("{}")
        });
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use futures_util::StreamExt;
    use tokio::sync::watch;
    use uuid::Uuid;

    use super::{LastEventId, stream_response};
    use crate::features::realtime::handlers::RealtimeScope;
    use crate::features::realtime::hub::{RealtimeEnvelope, RealtimeHub};

    #[tokio::test]
    async fn sse_stream_terminates_when_shutdown_fires() {
        let hub = RealtimeHub::new(4, false);
        let receiver = hub.subscribe();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let response =
            stream_response(receiver, shutdown_rx, 7, RealtimeScope::Public, None, Vec::new());
        let mut frames = response.into_body().into_data_stream();
        let connected = frames.next().await.expect("the connected frame must arrive");
        assert!(!connected.expect("connected frame bytes").is_empty());

        shutdown_tx.send(true).expect("shutdown channel must be open");
        assert!(frames.next().await.is_none(), "the stream must end once the shutdown watch fires");
    }

    #[tokio::test]
    async fn sse_stream_keeps_delivering_until_shutdown_fires() {
        let hub = RealtimeHub::new(4, false);
        let receiver = hub.subscribe();
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let response =
            stream_response(receiver, shutdown_rx, 7, RealtimeScope::Staff, None, Vec::new());
        let mut frames = response.into_body().into_data_stream();
        let _connected = frames.next().await.expect("the connected frame must arrive");

        let event = project_balloon_contracts::RealtimeEvent::connected(7, RealtimeScope::Staff);
        hub.publish(RealtimeEnvelope { event, team_id: None });
        assert!(
            frames.next().await.is_some(),
            "published in-scope events must still stream before shutdown"
        );

        shutdown_tx.send(true).expect("shutdown channel must be open");
        assert!(frames.next().await.is_none());
    }

    #[tokio::test]
    async fn last_event_id_is_read_from_header_then_query() {
        let uuid = Uuid::new_v4();
        let request =
            Request::builder().header("last-event-id", uuid.to_string()).body(()).expect("request");
        let (mut parts, _) = request.into_parts();
        assert_eq!(LastEventId::from_request_parts(&mut parts, &()).await.unwrap().0, Some(uuid));

        let request = Request::builder()
            .uri("/api/team/events/contests/7?lastEventId=ignored")
            .body(())
            .expect("request");
        let (mut parts, _) = request.into_parts();
        assert_eq!(LastEventId::from_request_parts(&mut parts, &()).await.unwrap().0, None);
    }

    #[tokio::test]
    async fn last_event_id_query_parameter_is_honored() {
        let uuid = Uuid::new_v4();
        let request = Request::builder()
            .uri(format!("/api/team/events/contests/7?lastEventId={uuid}"))
            .body(())
            .expect("request");
        let (mut parts, _) = request.into_parts();
        assert_eq!(LastEventId::from_request_parts(&mut parts, &()).await.unwrap().0, Some(uuid));

        let request = Request::builder()
            .uri("/api/team/events/contests/7?lastEventId=not-a-uuid")
            .body(())
            .expect("request");
        let (mut parts, _) = request.into_parts();
        assert_eq!(LastEventId::from_request_parts(&mut parts, &()).await.unwrap().0, None);

        let request = Request::builder().body(()).expect("request");
        let (mut parts, _) = request.into_parts();
        assert_eq!(LastEventId::from_request_parts(&mut parts, &()).await.unwrap().0, None);
    }

    async fn test_outbox() -> crate::features::realtime::outbox::RealtimeOutbox {
        let url = std::env::var("PROJECT_BALLOON_TEST_REDIS_URL")
            .expect("PROJECT_BALLOON_TEST_REDIS_URL is required");
        let handle =
            crate::features::redis::RedisHandle::connect(&url, Duration::from_millis(500))
                .await
                .expect("connect test Redis");
        crate::features::realtime::outbox::RealtimeOutbox::new(handle)
    }

    #[allow(clippy::too_many_arguments)]
    fn outbox_entry(
        contest_id: i64,
        scope: &str,
        team_id: Option<i64>,
        occurred_at_millis: i64,
        event_type: &str,
    ) -> crate::features::realtime::outbox::OutboxEntry {
        crate::features::realtime::outbox::OutboxEntry {
            event_id: Uuid::new_v4(),
            contest_id,
            event_type: event_type.to_owned(),
            schema_version: 1,
            scope: scope.to_owned(),
            team_id,
            occurred_at_millis,
            payload: serde_json::json!({}),
        }
    }

    #[tokio::test]
    #[ignore = "requires Redis reachable at PROJECT_BALLOON_TEST_REDIS_URL"]
    async fn replay_returns_in_scope_events_after_the_anchor() {
        let outbox = test_outbox().await;
        let contest_id = 7;
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1_000;
        let anchor = outbox_entry(contest_id, "TEAM", Some(12), now - 4 * 60 * 1000, "ANCHOR");
        let first = outbox_entry(contest_id, "TEAM", Some(12), now - 3 * 60 * 1000, "FIRST");
        let second = outbox_entry(contest_id, "TEAM", Some(12), now - 2 * 60 * 1000, "SECOND");
        // Excluded: other team and other scope.
        let other_team = outbox_entry(contest_id, "TEAM", Some(13), now - 150_000, "OTHER_TEAM");
        let other_scope = outbox_entry(contest_id, "STAFF", None, now - 140_000, "OTHER_SCOPE");
        for entry in [&anchor, &first, &second, &other_team, &other_scope] {
            outbox.enqueue(entry).await.expect("enqueue replay entry");
        }

        let replay = outbox
            .load_replay(contest_id, RealtimeScope::Team, Some(12), anchor.event_id)
            .await
            .expect("load replay");
        let ids: Vec<Uuid> = replay.iter().map(|event| event.id).collect();
        assert_eq!(ids, vec![first.event_id, second.event_id], "replay must follow publication order");
        assert!(replay.iter().all(|event| event.scope.as_str() == "TEAM"));
        assert!(replay.iter().all(|event| event.contest_id == contest_id));
    }

    #[tokio::test]
    #[ignore = "requires Redis reachable at PROJECT_BALLOON_TEST_REDIS_URL"]
    async fn replay_is_skipped_for_unknown_anchors() {
        let outbox = test_outbox().await;
        let contest_id = 8;
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1_000;
        let entry = outbox_entry(contest_id, "PUBLIC", None, now - 30_000, "CONTEST_AUTO_FROZEN");
        outbox.enqueue(&entry).await.expect("enqueue replay entry");

        let replay = outbox
            .load_replay(contest_id, RealtimeScope::Public, None, Uuid::new_v4())
            .await
            .expect("load replay with unknown anchor");
        assert!(replay.is_empty(), "an unknown anchor must yield no replay");
    }

    #[tokio::test]
    #[ignore = "requires Redis reachable at PROJECT_BALLOON_TEST_REDIS_URL"]
    async fn staff_replay_matches_only_teamless_events() {
        let outbox = test_outbox().await;
        let contest_id = 9;
        let now = time::OffsetDateTime::now_utc().unix_timestamp() * 1_000;
        let anchor = outbox_entry(contest_id, "STAFF", None, now - 4 * 60 * 1000, "ANCHOR");
        let teamless = outbox_entry(contest_id, "STAFF", None, now - 2 * 60 * 1000, "TEAMLESS");
        // TEAM entries always carry a team id, so a staff stream must never see them.
        let team_scoped = outbox_entry(contest_id, "TEAM", Some(14), now - 90_000, "TEAM_SCOPED");
        for entry in [&anchor, &teamless, &team_scoped] {
            outbox.enqueue(entry).await.expect("enqueue replay entry");
        }

        let replay = outbox
            .load_replay(contest_id, RealtimeScope::Staff, None, anchor.event_id)
            .await
            .expect("load staff replay");
        let ids: Vec<Uuid> = replay.iter().map(|event| event.id).collect();
        assert_eq!(ids, vec![teamless.event_id]);
    }
}
