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
use tokio::sync::broadcast;

use crate::{
    error::AppError,
    features::auth::{AuthContext, OptionalAuthContext, model::UserType},
    state::AppState,
};

use super::hub::RealtimeEnvelope;

#[utoipa::path(get, path = "/api/public/events/contests/{contest_id}", operation_id = "subscribePublicContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent public contest events", content_type = "text/event-stream", body = String), (status = 404, body = crate::error::ApiErrorBody)))]
pub async fn subscribe_public(
    context: OptionalAuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    state.contests().get(contest_id, context.user()).await?;
    Ok(stream_response(state.realtime().subscribe(), contest_id, RealtimeScope::Public, None))
}

#[utoipa::path(get, path = "/api/events/contests/{contest_id}", operation_id = "subscribeStaffContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent staff contest events", content_type = "text/event-stream", body = String), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn subscribe_staff(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    const STAFF_ROLES: [&str; 8] = [
        "CONTEST_ADMIN",
        "JUDGE",
        "BALLOON_STAFF",
        "PRINTER",
        "SCREEN_OPERATOR",
        "LIVE_OPERATOR",
        "RESOLVER_OPERATOR",
        "SUPER_ADMIN",
    ];
    if !STAFF_ROLES.iter().any(|role| context.user().has_role(role)) {
        return Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"));
    }
    state.contests().get(contest_id, Some(context.user())).await?;
    if !context.user().has_role("SUPER_ADMIN") {
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM contest_admin_assignments WHERE contest_id=$1 AND user_id=$2)",
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
    Ok(stream_response(state.realtime().subscribe(), contest_id, RealtimeScope::Staff, None))
}

#[utoipa::path(get, path = "/api/team/events/contests/{contest_id}", operation_id = "subscribeTeamContestEvents", tag = "realtime", params(("contest_id" = i64, Path)), responses((status = 200, description = "Server-sent team contest events", content_type = "text/event-stream", body = String), (status = 401, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn subscribe_team(
    context: AuthContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Response, AppError> {
    context.require_password_ready()?;
    if context.user().user_type != UserType::Team {
        return Err(AppError::not_found("CONTEST_NOT_FOUND", "Contest not found"));
    }
    let team_id = state.contests().require_team_id(contest_id, context.user().id).await?;
    Ok(stream_response(
        state.realtime().subscribe(),
        contest_id,
        RealtimeScope::Team,
        Some(team_id),
    ))
}

fn stream_response(
    receiver: broadcast::Receiver<RealtimeEnvelope>,
    contest_id: i64,
    scope: RealtimeScope,
    team_id: Option<i64>,
) -> Response {
    let connected =
        stream::once(async move { event_frame(&RealtimeEvent::connected(contest_id, scope)) });
    let messages = stream::unfold(receiver, move |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(envelope)
                    if envelope.event.contest_id == contest_id
                        && envelope.event.scope == scope
                        && envelope.team_id == team_id =>
                {
                    return Some((event_frame(&envelope.event), receiver));
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
    });
    let stream = connected.chain(messages);
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
