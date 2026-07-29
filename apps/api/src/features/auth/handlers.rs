use std::{net::SocketAddr, time::Duration as StdDuration};

use axum::{
    Json,
    extract::{ConnectInfo, State, rejection::JsonRejection},
    http::StatusCode,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::{error::AppError, state::AppState};

use super::{
    SESSION_COOKIE_NAME,
    context::AuthContext,
    model::{ChangePasswordRequest, CurrentUserResponse, LoginRequest, RegisterRequest},
};

#[utoipa::path(
    post,
    path = "/api/auth/login",
    operation_id = "login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated user; PB_SESSION is set as an HttpOnly cookie", body = CurrentUserResponse),
        (status = 400, description = "Malformed or invalid credentials payload", body = crate::error::ApiErrorBody),
        (status = 401, description = "Invalid username or password", body = crate::error::ApiErrorBody),
        (status = 403, description = "CSRF token is missing or invalid", body = crate::error::ApiErrorBody),
        (status = 429, description = "Too many login attempts", body = crate::error::ApiErrorBody)
    ),
    security(("csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<(CookieJar, Json<CurrentUserResponse>), AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid login JSON object"))?;
    let outcome = state.auth().login(request, peer.ip()).await?;
    if let Some(previous) = jar.get(SESSION_COOKIE_NAME) {
        state.auth().logout_token(previous.value()).await?;
    }
    let cookie = session_cookie(
        outcome.session_token,
        state.auth().session_ttl(),
        state.auth().secure_cookies(),
    );
    Ok((jar.add(cookie), Json(outcome.user.response())))
}

#[utoipa::path(post, path = "/api/auth/register", operation_id = "registerIndividual", tag = "auth", request_body = RegisterRequest, responses((status = 200, body = CurrentUserResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("csrf_cookie" = [], "csrf_header" = [])))]
pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    payload: Result<Json<RegisterRequest>, JsonRejection>,
) -> Result<(CookieJar, Json<CurrentUserResponse>), AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid registration object"))?;
    let outcome = state.auth().register(request, peer.ip()).await?;
    let cookie = session_cookie(
        outcome.session_token,
        state.auth().session_ttl(),
        state.auth().secure_cookies(),
    );
    Ok((jar.add(cookie), Json(outcome.user.response())))
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    operation_id = "currentUser",
    tag = "auth",
    responses(
        (status = 200, description = "Current authenticated user", body = CurrentUserResponse),
        (status = 401, description = "Session is missing, expired, disabled, or stale", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = []))
)]
pub async fn current_user(context: AuthContext) -> Json<CurrentUserResponse> {
    Json(context.user().response())
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    operation_id = "logout",
    tag = "auth",
    responses(
        (status = 204, description = "Session revoked and PB_SESSION expired"),
        (status = 401, description = "Authentication required", body = crate::error::ApiErrorBody),
        (status = 403, description = "CSRF token is missing or invalid", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn logout(
    context: AuthContext,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    state.auth().logout(&context.session().token_hash).await?;
    Ok((jar.remove(expired_session_cookie()), StatusCode::NO_CONTENT))
}

#[utoipa::path(
    post,
    path = "/api/auth/password",
    operation_id = "changePassword",
    tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed and mandatory reset flag cleared", body = CurrentUserResponse),
        (status = 400, description = "Invalid request, current password, or unchanged password", body = crate::error::ApiErrorBody),
        (status = 401, description = "Authentication required or account disabled", body = crate::error::ApiErrorBody),
        (status = 403, description = "CSRF token is missing or invalid", body = crate::error::ApiErrorBody)
    ),
    security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = []))
)]
pub async fn change_password(
    context: AuthContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    payload: Result<Json<ChangePasswordRequest>, JsonRejection>,
) -> Result<Json<CurrentUserResponse>, AppError> {
    let Json(request) = payload.map_err(|_| {
        AppError::validation("request", "must be a valid password-change JSON object")
    })?;
    let user = state.auth().change_password(context.session(), request, peer.ip()).await?;
    Ok(Json(user.response()))
}

fn session_cookie(token: String, ttl: StdDuration, secure: bool) -> Cookie<'static> {
    let max_age_seconds = i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX);
    Cookie::build((SESSION_COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .max_age(cookie_duration(max_age_seconds))
        .build()
}

fn expired_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie_duration(0))
        .build()
}

fn cookie_duration(seconds: i64) -> time::Duration {
    time::Duration::seconds(seconds)
}
