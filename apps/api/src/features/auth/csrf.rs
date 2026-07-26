use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use utoipa::ToSchema;

use axum::{
    Json,
    extract::{Request, State},
    http::{Method, header::HeaderName},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::{error::AppError, state::AppState};

pub const CSRF_COOKIE_NAME: &str = "XSRF-TOKEN";
pub const CSRF_HEADER_NAME: &str = "X-XSRF-TOKEN";

type HmacSha256 = Hmac<Sha256>;

pub struct CsrfSigner {
    secret: Vec<u8>,
}

impl CsrfSigner {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        Self { secret: secret.to_vec() }
    }

    pub fn issue(&self) -> Result<String, AppError> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce)
            .map_err(|error| AppError::internal("generate CSRF token", error))?;
        let nonce = URL_SAFE_NO_PAD.encode(nonce);
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|error| AppError::internal("initialize CSRF signer", error))?;
        mac.update(nonce.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{nonce}.{signature}"))
    }

    #[must_use]
    pub fn verify(&self, token: &str) -> bool {
        if token.len() > 256 {
            return false;
        }
        let Some((nonce, signature)) = token.split_once('.') else {
            return false;
        };
        let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
            return false;
        };
        let Ok(mut mac) = HmacSha256::new_from_slice(&self.secret) else {
            return false;
        };
        mac.update(nonce.as_bytes());
        mac.verify_slice(&signature).is_ok()
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CsrfResponse {
    header_name: &'static str,
    parameter_name: &'static str,
    token: String,
}

#[utoipa::path(
    get,
    path = "/api/auth/csrf",
    operation_id = "csrf",
    tag = "auth",
    responses(
        (status = 200, description = "CSRF token issued; the same token is also set in the XSRF-TOKEN cookie", body = CsrfResponse),
        (status = 500, description = "CSRF token generation failed", body = crate::error::ApiErrorBody)
    )
)]
pub async fn csrf(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<CsrfResponse>), AppError> {
    let token = state.csrf().issue()?;
    let cookie = Cookie::build((CSRF_COOKIE_NAME, token.clone()))
        .path("/")
        .http_only(false)
        .secure(state.auth().secure_cookies())
        .same_site(SameSite::Lax)
        .build();
    let body = CsrfResponse { header_name: CSRF_HEADER_NAME, parameter_name: "_csrf", token };
    Ok((jar.add(cookie), Json(body)))
}

pub async fn protect_csrf(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(request).await;
    }

    let jar = CookieJar::from_headers(request.headers());
    let cookie_token = jar.get(CSRF_COOKIE_NAME).map(|cookie| cookie.value());
    let header_token = request
        .headers()
        .get(HeaderName::from_static("x-xsrf-token"))
        .and_then(|value| value.to_str().ok());
    let valid = match (cookie_token, header_token) {
        (Some(cookie), Some(header)) => {
            state.csrf().verify(cookie) && cookie.as_bytes().ct_eq(header.as_bytes()).into()
        }
        _ => false,
    };
    if !valid {
        return AppError::forbidden("CSRF_INVALID", "CSRF token is missing or invalid")
            .into_response();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::CsrfSigner;

    #[test]
    fn issued_token_verifies_and_tampering_fails() {
        let signer = CsrfSigner::new(b"test-secret-that-is-longer-than-32-bytes");
        let token = signer.issue().expect("token must be generated");

        assert!(signer.verify(&token));
        assert!(!signer.verify(&format!("{token}x")));
    }
}
