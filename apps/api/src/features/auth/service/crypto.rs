use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::features::auth::model::AuthUser;

use crate::features::auth::service::SESSION_TOKEN_BYTES;

pub(super) fn random_token() -> Result<String, AppError> {
    let mut bytes = [0_u8; SESSION_TOKEN_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| AppError::internal("generate session token", error))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
pub(super) fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub(super) fn access_fingerprint(user: &AuthUser) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user.user_type.as_str().as_bytes());
    for permission in &user.permissions {
        hasher.update([0]);
        hasher.update(permission.as_bytes());
    }
    hex::encode(hasher.finalize())
}
pub(super) fn constant_time_equal(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}
pub(super) fn invalid_credentials() -> AppError {
    AppError::unauthorized("INVALID_CREDENTIALS", "Invalid username or password")
}
pub(super) fn rate_limited() -> AppError {
    AppError::too_many_requests("RATE_LIMIT_EXCEEDED", "Too many login attempts; try again later")
}
pub(super) fn not_authenticated() -> AppError {
    AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated")
}
