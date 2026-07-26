use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::cookie::CookieJar;

use crate::{error::AppError, state::AppState};

use super::{SESSION_COOKIE_NAME, model::AuthUser, service::AuthenticatedSession};

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
        if self.user().password_reset_required {
            Err(AppError::forbidden("PASSWORD_RESET_REQUIRED", "Password change is required"))
        } else {
            Ok(())
        }
    }
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE_NAME)
            .map(|cookie| cookie.value())
            .ok_or_else(|| AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated"))?;
        let session = state.auth().authenticate(token).await?;
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
        let Some(token) = jar.get(SESSION_COOKIE_NAME).map(|cookie| cookie.value()) else {
            return Ok(Self { session: None });
        };
        let session = state.auth().authenticate(token).await?;
        Ok(Self { session: Some(session) })
    }
}
