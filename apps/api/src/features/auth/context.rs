use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};
use axum_extra::extract::cookie::CookieJar;

use crate::{error::AppError, state::AppState};

use super::{
    SESSION_COOKIE_NAME,
    model::{AuthUser, CurrentUserResponse},
    service::AuthenticatedSession,
};

#[derive(Debug)]
pub struct AuthContext {
    session: AuthenticatedSession,
}

#[derive(Debug)]
pub struct SuperAdminContext {
    inner: AuthContext,
}

#[derive(Debug)]
pub struct ContestManagerContext {
    inner: AuthContext,
}

#[derive(Debug)]
pub struct OptionalAuthContext {
    session: Option<AuthenticatedSession>,
}

impl SuperAdminContext {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        self.inner.user()
    }
}

impl ContestManagerContext {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        self.inner.user()
    }
}

impl OptionalAuthContext {
    #[must_use]
    pub fn user(&self) -> Option<&AuthUser> {
        self.session.as_ref().map(|session| &session.user)
    }
}

impl AuthContext {
    #[must_use]
    pub const fn user(&self) -> &AuthUser {
        &self.session.user
    }

    #[must_use]
    pub const fn session(&self) -> &AuthenticatedSession {
        &self.session
    }

    pub fn require_role(&self, role: &'static str) -> Result<(), AppError> {
        if self.user().has_role(role) {
            Ok(())
        } else {
            Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"))
        }
    }

    pub fn require_password_ready(&self) -> Result<(), AppError> {
        if self.session.workstation_binding_id.is_some() {
            return Ok(());
        }
        if self.user().password_reset_required {
            Err(AppError::forbidden("PASSWORD_RESET_REQUIRED", "Password change is required"))
        } else {
            Ok(())
        }
    }

    pub fn require_account_session(&self) -> Result<(), AppError> {
        if self.session.workstation_binding_id.is_some() {
            Err(AppError::forbidden(
                "WORKSTATION_SESSION_RESTRICTED",
                "This action requires an account login",
            ))
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn response(&self) -> CurrentUserResponse {
        let mut response = self.user().response();
        response.competition = self.session.competition.clone();
        if response.competition.is_some() {
            response.password_reset_required = false;
        }
        response
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<AuthenticatedSession, AppError> {
    let jar = CookieJar::from_headers(&parts.headers);
    let token = jar
        .get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value())
        .ok_or_else(|| AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated"))?;
    let mut session = state.auth().authenticate(token).await?;
    if let Some(binding_id) = session.workstation_binding_id {
        let request_ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|peer| peer.0.ip())
            .ok_or_else(|| AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated"))?;
        if session.bound_ip.as_deref() != Some(request_ip.to_string().as_str()) {
            state.auth().logout(&session.token_hash).await?;
            return Err(AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated"));
        }
        match state
            .competition()
            .validate_session(state.deployment_mode(), binding_id, session.user.id, request_ip)
            .await
        {
            Ok(competition) => session.competition = Some(competition),
            Err(error) => {
                state.auth().logout(&session.token_hash).await?;
                return Err(error);
            }
        }
    }
    Ok(session)
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = authenticate(parts, state).await?;
        Ok(Self { session })
    }
}

impl FromRequestParts<AppState> for SuperAdminContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let inner = AuthContext::from_request_parts(parts, state).await?;
        inner.require_password_ready()?;
        inner.require_role("SUPER_ADMIN")?;
        Ok(Self { inner })
    }
}

impl FromRequestParts<AppState> for ContestManagerContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let inner = AuthContext::from_request_parts(parts, state).await?;
        inner.require_password_ready()?;
        if inner.user().has_role("SUPER_ADMIN") || inner.user().has_role("CONTEST_ADMIN") {
            Ok(Self { inner })
        } else {
            Err(AppError::forbidden("FORBIDDEN", "Insufficient permissions"))
        }
    }
}

impl FromRequestParts<AppState> for OptionalAuthContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        if jar.get(SESSION_COOKIE_NAME).is_none() {
            return Ok(Self { session: None });
        }
        let session = authenticate(parts, state).await?;
        Ok(Self { session: Some(session) })
    }
}
