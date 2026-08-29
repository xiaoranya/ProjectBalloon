mod context;
pub(crate) mod csrf;
pub(crate) mod handlers;
pub mod model;
mod password;
pub mod permissions;
mod service;

pub use context::{AuthContext, ContestManagerContext, OptionalAuthContext, SuperAdminContext};
pub use csrf::{CsrfSigner, csrf, protect_csrf};
pub use handlers::{
    change_password, current_user, login, logout, register, update_profile, workstation_login,
};
pub(crate) use password::hash as hash_password;
pub use service::AuthService;

pub const SESSION_COOKIE_NAME: &str = "PB_SESSION";

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/api/auth/csrf", get(csrf))
        .route("/api/auth/login", post(login))
        .route("/api/auth/workstation", post(workstation_login))
        .route("/api/auth/register", post(register))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(current_user))
        .route("/api/auth/profile", patch(update_profile))
        .route("/api/auth/password", post(change_password))
}

use axum::routing::{get, patch, post};
