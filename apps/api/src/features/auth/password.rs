use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use std::sync::OnceLock;
use thiserror::Error;
use tokio::sync::Semaphore;

// Argon2's default memory cost is about 19 MiB. An unbounded spawn_blocking
// burst creates many glibc arenas which can retain several GiB after login
// traffic subsides. Eight permits preserve normal login throughput while
// bounding the active password-hashing working set.
const PASSWORD_HASHING_CONCURRENCY: usize = 8;
static PASSWORD_HASHING_PERMITS: Semaphore = Semaphore::const_new(PASSWORD_HASHING_CONCURRENCY);

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password hashing worker failed: {0}")]
    Worker(#[from] tokio::task::JoinError),
    #[error("password hashing capacity is unavailable")]
    Capacity,
    #[error("password hash is invalid")]
    InvalidHash,
    #[error("operating-system randomness is unavailable")]
    Random,
    #[error("password hashing failed")]
    Hash,
}

pub async fn hash(password: String) -> Result<String, PasswordError> {
    let _permit = PASSWORD_HASHING_PERMITS.acquire().await.map_err(|_| PasswordError::Capacity)?;
    tokio::task::spawn_blocking(move || hash_blocking(&password)).await?
}

pub async fn verify(password: String, encoded: String) -> Result<bool, PasswordError> {
    let _permit = PASSWORD_HASHING_PERMITS.acquire().await.map_err(|_| PasswordError::Capacity)?;
    tokio::task::spawn_blocking(move || verify_blocking(&password, &encoded)).await?
}

pub async fn verify_dummy(password: String) -> Result<bool, PasswordError> {
    verify(password, dummy_argon_hash().to_owned()).await
}

#[must_use]
pub fn needs_upgrade(encoded: &str) -> bool {
    !encoded.starts_with("$argon2id$")
}

fn hash_blocking(password: &str) -> Result<String, PasswordError> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|_| PasswordError::Random)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| PasswordError::Hash)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| PasswordError::Hash)
}

fn verify_blocking(password: &str, encoded: &str) -> Result<bool, PasswordError> {
    if encoded.starts_with("$2") {
        let matches = bcrypt::verify(password, encoded).map_err(|_| PasswordError::InvalidHash)?;
        let dummy =
            PasswordHash::new(dummy_argon_hash()).map_err(|_| PasswordError::InvalidHash)?;
        let _ = Argon2::default().verify_password(password.as_bytes(), &dummy);
        return Ok(matches);
    }
    let parsed = PasswordHash::new(encoded).map_err(|_| PasswordError::InvalidHash)?;
    let matches = Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok();
    let _ = bcrypt::verify(password, dummy_bcrypt_hash());
    Ok(matches)
}

fn dummy_argon_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_blocking("project-balloon-dummy-password").expect("dummy hash"))
        .as_str()
}

fn dummy_bcrypt_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| bcrypt::hash("project-balloon-dummy-password", 4).expect("dummy hash"))
}

#[cfg(test)]
mod tests {
    use crate::features::auth::password::{hash, needs_upgrade, verify};

    #[tokio::test]
    async fn argon2id_round_trip() {
        let encoded =
            hash("correct horse battery staple".to_owned()).await.expect("password must hash");

        assert!(encoded.starts_with("$argon2id$"));
        assert!(
            verify("correct horse battery staple".to_owned(), encoded.clone())
                .await
                .expect("hash must verify")
        );
        assert!(
            !verify("wrong".to_owned(), encoded)
                .await
                .expect("wrong password is a normal mismatch")
        );
    }

    #[tokio::test]
    async fn legacy_bcrypt_is_supported_and_marked_for_upgrade() {
        let encoded = bcrypt::hash("legacy-password", 4).expect("bcrypt fixture must hash");

        assert!(needs_upgrade(&encoded));
        assert!(
            verify("legacy-password".to_owned(), encoded).await.expect("legacy hash must verify")
        );
    }
}
