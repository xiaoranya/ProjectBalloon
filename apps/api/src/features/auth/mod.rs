mod context;
pub(crate) mod csrf;
pub(crate) mod handlers;
pub mod model;
mod password;
mod service;

pub use context::{AuthContext, ContestManagerContext, OptionalAuthContext, SuperAdminContext};
pub use csrf::{CsrfSigner, csrf, protect_csrf};
pub use handlers::{
    change_password, current_user, login, logout, register, update_profile, workstation_login,
};
pub(crate) use password::hash as hash_password;
pub use service::AuthService;

pub const SESSION_COOKIE_NAME: &str = "PB_SESSION";
