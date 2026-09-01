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
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::{AuthContext, OptionalAuthContext, model::UserType},
    state::AppState,
};

use crate::features::realtime::hub::RealtimeEnvelope;

/// Replay window for Last-Event-ID resume: at most this many events and never
/// older than this window, whichever limit hits first. Older gaps fall back to
/// the client's poll-based full refresh.
const REPLAY_MAX_EVENTS: i64 = 100;
const REPLAY_WINDOW: &str = "5 minutes";

#[utoipa::path(get, path = "/api/public/events/contests/{contest_id}", operation_id = "subscribePublicContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent public contest events", content_type = "text/event-stream", body = String), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn subscribe_public(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
    last_event_id: LastEventId,
) -> Result<Response, AppError> {
    state.contests().get(contest_id, context.user()).await?;
    let replay =
        load_replay(state.database(), contest_id, RealtimeScope::Public, None, last_event_id.0)
            .await?;
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
    let replay =
        load_replay(state.database(), contest_id, RealtimeScope::Staff, None, last_event_id.0)
            .await?;
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
    let replay = load_replay(
        state.database(),
        contest_id,
        RealtimeScope::Team,
        Some(team_id),
        last_event_id.0,
    )
    .await?;
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

#[derive(Debug, sqlx::FromRow)]
struct ReplayRow {
    event_id: Uuid,
    event_type: String,
    schema_version: i16,
    scope: String,
    payload_json: Value,
    created_at: time::OffsetDateTime,
}

/// Replays published outbox events that follow `last_event_id`, honoring the
/// stream's scope/team filtering and bounded by the recent window and a hard
/// cap. An unknown id, an id outside the window, or an empty history simply
/// yields no replay — the client's poll refresh covers gaps.
async fn load_replay(
    database: &sqlx::PgPool,
    contest_id: i64,
    scope: RealtimeScope,
    team_id: Option<i64>,
    last_event_id: Option<Uuid>,
) -> Result<Vec<RealtimeEvent>, AppError> {
    let Some(last_event_id) = last_event_id else { return Ok(Vec::new()) };
    let rows = sqlx::query_as::<_, ReplayRow>(
        r#"
        WITH anchor AS (
            SELECT created_at FROM realtime_outbox
            WHERE event_id = $1 AND contest_id = $2
        )
        SELECT o.event_id, o.event_type, o.schema_version, o.scope,
               o.payload_json, o.created_at
        FROM realtime_outbox o
        JOIN anchor ON o.created_at > anchor.created_at
        WHERE o.contest_id = $2
          AND o.scope = $3
          AND o.team_id IS NOT DISTINCT FROM $4
          AND o.status = 'PUBLISHED'
          AND o.created_at >= now() - $5::interval
        ORDER BY o.created_at DESC, o.id DESC
        LIMIT $6
        "#,
    )
    .bind(last_event_id)
    .bind(contest_id)
    .bind(scope.as_str())
    .bind(team_id)
    .bind(REPLAY_WINDOW)
    .bind(REPLAY_MAX_EVENTS)
    .fetch_all(database)
    .await
    .map_err(|error| {
        AppError::internal("load realtime replay", error).with_contest_id(contest_id)
    })?;
    let mut events: Vec<RealtimeEvent> = rows
        .into_iter()
        .filter_map(|row| {
            let scope = super::dispatcher::parse_scope(&row.scope)?;
            Some(RealtimeEvent {
                id: row.event_id,
                version: row.schema_version.cast_unsigned(),
                event_type: row.event_type,
                scope,
                contest_id,
                occurred_at: row.created_at,
                payload: row.payload_json,
            })
        })
        .collect();
    // Selected newest-first for the LIMIT; replay must flow in publication order.
    events.reverse();
    Ok(events)
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
    use axum::extract::FromRequestParts;
    use axum::http::Request;
    use futures_util::StreamExt;
    use sqlx::PgPool;
    use tokio::sync::watch;
    use uuid::Uuid;

    use super::{LastEventId, load_replay, stream_response};
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

    async fn seed_contest(pool: &sqlx::PgPool) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Replay Contest', 'RUNNING', 'PRIVATE') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("insert contest")
    }

    async fn seed_team(pool: &sqlx::PgPool, name: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ($1) RETURNING id")
            .bind(format!("Replay Team {name}"))
            .fetch_one(pool)
            .await
            .expect("insert team")
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_event(
        pool: &sqlx::PgPool,
        contest_id: i64,
        scope: &str,
        team_id: Option<i64>,
        status: &str,
        created_ago: &str,
        event_type: &str,
    ) -> Uuid {
        let event_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO realtime_outbox
                (event_id, contest_id, event_type, scope, team_id, payload_json, status,
                 created_at, published_at)
            VALUES ($1, $2, $3, $4, $5, '{}'::jsonb, $6, now() - $7::interval, now())
            "#,
        )
        .bind(event_id)
        .bind(contest_id)
        .bind(event_type)
        .bind(scope)
        .bind(team_id)
        .bind(status)
        .bind(created_ago)
        .execute(pool)
        .await
        .expect("insert outbox event");
        event_id
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn replay_returns_published_in_scope_events_after_the_anchor(pool: PgPool) {
        let contest_id = seed_contest(&pool).await;
        let team_id = seed_team(&pool, "One").await;
        let other_team_id = seed_team(&pool, "Two").await;
        let anchor = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PUBLISHED",
            "30 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;
        // In window, after the anchor, in scope: both must replay in order.
        let first = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PUBLISHED",
            "4 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;
        let second = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PUBLISHED",
            "2 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;
        // Excluded: other team, other scope, unpublished, and outside the window.
        let _other_team = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(other_team_id),
            "PUBLISHED",
            "3 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;
        let _other_scope = seed_event(
            &pool,
            contest_id,
            "STAFF",
            None,
            "PUBLISHED",
            "3 minutes",
            "ANNOUNCEMENT_PUBLISHED",
        )
        .await;
        let _unpublished = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PENDING",
            "3 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;
        let _outside_window = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PUBLISHED",
            "10 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;

        let replay =
            load_replay(&pool, contest_id, RealtimeScope::Team, Some(team_id), Some(anchor))
                .await
                .expect("load replay");
        let ids: Vec<Uuid> = replay.iter().map(|event| event.id).collect();
        assert_eq!(ids, vec![first, second], "replay must follow publication order");
        assert!(replay.iter().all(|event| event.scope.as_str() == "TEAM"));
        assert!(replay.iter().all(|event| event.contest_id == contest_id));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn replay_is_skipped_for_unknown_anchors(pool: PgPool) {
        let contest_id = seed_contest(&pool).await;
        seed_event(
            &pool,
            contest_id,
            "PUBLIC",
            None,
            "PUBLISHED",
            "1 minutes",
            "CONTEST_AUTO_FROZEN",
        )
        .await;
        let replay =
            load_replay(&pool, contest_id, RealtimeScope::Public, None, Some(Uuid::new_v4()))
                .await
                .expect("load replay with unknown anchor");
        assert!(replay.is_empty());

        let replay = load_replay(&pool, contest_id, RealtimeScope::Public, None, None)
            .await
            .expect("load replay without anchor");
        assert!(replay.is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn staff_replay_matches_only_teamless_events(pool: PgPool) {
        let contest_id = seed_contest(&pool).await;
        let anchor = seed_event(
            &pool,
            contest_id,
            "STAFF",
            None,
            "PUBLISHED",
            "30 minutes",
            "SUBMISSION_REJUDGED",
        )
        .await;
        let teamless = seed_event(
            &pool,
            contest_id,
            "STAFF",
            None,
            "PUBLISHED",
            "2 minutes",
            "ANNOUNCEMENT_PUBLISHED",
        )
        .await;
        // TEAM rows always carry a team id, so a staff stream must never see them.
        let team_id = seed_team(&pool, "Staff Side").await;
        let _team_scoped = seed_event(
            &pool,
            contest_id,
            "TEAM",
            Some(team_id),
            "PUBLISHED",
            "2 minutes",
            "SUBMISSION_STATUS_CHANGED",
        )
        .await;

        let replay = load_replay(&pool, contest_id, RealtimeScope::Staff, None, Some(anchor))
            .await
            .expect("load staff replay");
        let ids: Vec<Uuid> = replay.iter().map(|event| event.id).collect();
        assert_eq!(ids, vec![teamless]);
    }
}
