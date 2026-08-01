use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
    Client,
    config::{Credentials, Region},
};
use bytes::{Bytes, BytesMut};
use project_balloon_contracts::JudgeTask;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{io::AsyncReadExt, time::timeout};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("artifact request timed out")]
    Timeout,
    #[error("artifact request failed: {0}")]
    Request(String),
    #[error("artifact exceeds configured maximum of {0} bytes")]
    TooLarge(u64),
    #[error("{kind} SHA-256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { kind: &'static str, expected: String, actual: String },
    #[error("artifact cache I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait ArtifactSource: Send + Sync {
    async fn check_bucket(&self, bucket: &str) -> Result<(), ArtifactError>;
    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ArtifactError>;
}

pub struct S3ArtifactSource {
    client: Client,
    request_timeout: Duration,
}

const MAX_S3_RESPONSE_BYTES: i64 = 512 * 1024 * 1024;

fn declared_size_is_allowed(length: Option<i64>) -> bool {
    length.is_none_or(|length| (0..=MAX_S3_RESPONSE_BYTES).contains(&length))
}

pub struct S3ArtifactSourceConfig {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub request_timeout: Duration,
}

impl S3ArtifactSource {
    #[must_use]
    pub fn new(config: S3ArtifactSourceConfig) -> Self {
        let credentials = Credentials::new(
            config.access_key,
            config.secret_key,
            None,
            None,
            "project-balloon-worker-static",
        );
        let sdk_config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(Region::new(config.region))
            .credentials_provider(credentials)
            .endpoint_url(config.endpoint)
            .force_path_style(true)
            .build();
        Self { client: Client::from_conf(sdk_config), request_timeout: config.request_timeout }
    }
}

#[async_trait]
impl ArtifactSource for S3ArtifactSource {
    async fn check_bucket(&self, bucket: &str) -> Result<(), ArtifactError> {
        timeout(self.request_timeout, self.client.head_bucket().bucket(bucket).send())
            .await
            .map_err(|_| ArtifactError::Timeout)?
            .map(|_| ())
            .map_err(|error| ArtifactError::Request(error.to_string()))
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ArtifactError> {
        let response =
            timeout(self.request_timeout, self.client.get_object().bucket(bucket).key(key).send())
                .await
                .map_err(|_| ArtifactError::Timeout)?
                .map_err(|error| ArtifactError::Request(error.to_string()))?;
        if !declared_size_is_allowed(response.content_length()) {
            return Err(ArtifactError::TooLarge(MAX_S3_RESPONSE_BYTES as u64));
        }
        let mut stream = response.body;
        let mut body = BytesMut::new();
        loop {
            let chunk = timeout(self.request_timeout, stream.next())
                .await
                .map_err(|_| ArtifactError::Timeout)?
                .transpose()
                .map_err(|error| ArtifactError::Request(error.to_string()))?;
            let Some(chunk) = chunk else { break };
            if body.len().saturating_add(chunk.len()) > MAX_S3_RESPONSE_BYTES as usize {
                return Err(ArtifactError::TooLarge(MAX_S3_RESPONSE_BYTES as u64));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body.freeze())
    }
}

#[derive(Debug)]
pub struct PreparedArtifacts {
    pub source: Bytes,
    pub testdata_archive: PathBuf,
    pub interactor: Option<Bytes>,
}

#[derive(Clone)]
pub struct ArtifactManager {
    source: Arc<dyn ArtifactSource>,
    cache_dir: PathBuf,
    problem_bucket: String,
    source_bucket: String,
    max_artifact_bytes: u64,
}

impl ArtifactManager {
    #[must_use]
    pub fn new(
        source: Arc<dyn ArtifactSource>,
        cache_dir: PathBuf,
        problem_bucket: String,
        source_bucket: String,
        max_artifact_bytes: u64,
    ) -> Self {
        Self { source, cache_dir, problem_bucket, source_bucket, max_artifact_bytes }
    }

    pub async fn preflight(&self) -> Result<(), ArtifactError> {
        create_private_dir(&self.cache_dir).await?;
        create_private_dir(&self.testdata_cache_dir()).await?;
        self.source.check_bucket(&self.problem_bucket).await?;
        if self.source_bucket != self.problem_bucket {
            self.source.check_bucket(&self.source_bucket).await?;
        }
        Ok(())
    }

    pub async fn prepare(&self, task: &JudgeTask) -> Result<PreparedArtifacts, ArtifactError> {
        let source = self.source.get(&self.source_bucket, &task.source_object_key).await?;
        self.validate_size(source.len())?;
        verify_hash("source", &source, &task.source_sha256)?;
        let testdata_archive = self.cached_testdata(task).await?;
        let interactor = match (&task.interactor_object_key, &task.interactor_sha256) {
            (Some(key), Some(expected)) => {
                let content = self.source.get(&self.problem_bucket, key).await?;
                self.validate_size(content.len())?;
                verify_hash("interactor", &content, expected)?;
                Some(content)
            }
            (None, None) => None,
            _ => return Err(ArtifactError::Request("incomplete interactor reference".to_owned())),
        };
        Ok(PreparedArtifacts { source, testdata_archive, interactor })
    }

    fn testdata_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("testdata")
    }

    fn testdata_path(&self, task: &JudgeTask) -> PathBuf {
        self.testdata_cache_dir().join(format!(
            "{}-v{}-{}.zip",
            task.problem_id, task.testdata_version, task.testdata_sha256
        ))
    }

    async fn cached_testdata(&self, task: &JudgeTask) -> Result<PathBuf, ArtifactError> {
        let path = self.testdata_path(task);
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            self.validate_size_u64(metadata.len())?;
            match verify_file_hash(&path, &task.testdata_sha256, self.max_artifact_bytes).await {
                Ok(()) => return Ok(path),
                Err(ArtifactError::HashMismatch { .. }) => {}
                Err(error) => return Err(error),
            }
            remove_if_present(&path).await?;
        }

        let content = self.source.get(&self.problem_bucket, &task.testdata_object_key).await?;
        self.validate_size(content.len())?;
        verify_hash("test data", &content, &task.testdata_sha256)?;
        let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        tokio::fs::write(&temporary, &content).await?;
        set_private_file_permissions(&temporary).await?;
        tokio::fs::rename(&temporary, &path).await?;
        Ok(path)
    }

    fn validate_size(&self, bytes: usize) -> Result<(), ArtifactError> {
        let bytes =
            u64::try_from(bytes).map_err(|_| ArtifactError::TooLarge(self.max_artifact_bytes))?;
        self.validate_size_u64(bytes)
    }

    fn validate_size_u64(&self, bytes: u64) -> Result<(), ArtifactError> {
        if bytes > self.max_artifact_bytes {
            return Err(ArtifactError::TooLarge(self.max_artifact_bytes));
        }
        Ok(())
    }
}

fn verify_hash(kind: &'static str, content: &[u8], expected: &str) -> Result<(), ArtifactError> {
    let actual = hex::encode(Sha256::digest(content));
    if actual != expected {
        return Err(ArtifactError::HashMismatch { kind, expected: expected.to_owned(), actual });
    }
    Ok(())
}

async fn verify_file_hash(
    path: &std::path::Path,
    expected: &str,
    max_bytes: u64,
) -> Result<(), ArtifactError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > max_bytes {
            return Err(ArtifactError::TooLarge(max_bytes));
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != expected {
        return Err(ArtifactError::HashMismatch {
            kind: "test data",
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}

async fn remove_if_present(path: &std::path::Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn create_private_dir(path: &std::path::Path) -> Result<(), std::io::Error> {
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700)).await?;
    Ok(())
}

async fn set_private_file_permissions(path: &std::path::Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use project_balloon_test_support::valid_judge_task;

    use super::*;

    #[derive(Default)]
    struct MemorySource {
        objects: HashMap<(String, String), Bytes>,
        reads: Mutex<HashMap<(String, String), usize>>,
    }

    #[async_trait]
    impl ArtifactSource for MemorySource {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ArtifactError> {
            Ok(())
        }

        async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ArtifactError> {
            let object = (bucket.to_owned(), key.to_owned());
            *self.reads.lock().expect("read count lock").entry(object.clone()).or_default() += 1;
            self.objects
                .get(&object)
                .cloned()
                .ok_or_else(|| ArtifactError::Request("not found".to_owned()))
        }
    }

    #[tokio::test]
    async fn source_is_verified_and_testdata_is_reused_by_hash() {
        let source = Bytes::from_static(b"int main() { return 0; }");
        let testdata = Bytes::from_static(b"fixture-testdata-archive");
        let source_hash = hex::encode(Sha256::digest(&source));
        let testdata_hash = hex::encode(Sha256::digest(&testdata));
        let mut task = valid_judge_task();
        task.source_sha256.clone_from(&source_hash);
        task.testdata_sha256.clone_from(&testdata_hash);
        let memory = Arc::new(MemorySource {
            objects: HashMap::from([
                (("sources".to_owned(), task.source_object_key.clone()), source.clone()),
                (("problems".to_owned(), task.testdata_object_key.clone()), testdata),
            ]),
            reads: Mutex::default(),
        });
        let cache = std::env::temp_dir().join(format!("project-balloon-worker-{}", Uuid::new_v4()));
        let manager = ArtifactManager::new(
            memory.clone(),
            cache.clone(),
            "problems".to_owned(),
            "sources".to_owned(),
            1024,
        );
        manager.preflight().await.expect("preflight");

        let first = manager.prepare(&task).await.expect("first acquisition");
        let second = manager.prepare(&task).await.expect("cached acquisition");
        assert_eq!(first.source, source);
        assert_eq!(first.testdata_archive, second.testdata_archive);
        assert_eq!(
            tokio::fs::read(first.testdata_archive).await.expect("cache read"),
            b"fixture-testdata-archive"
        );
        {
            let reads = memory.reads.lock().expect("read count lock");
            assert_eq!(reads.get(&("problems".to_owned(), task.testdata_object_key)), Some(&1));
        }
        tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
    }

    #[test]
    fn declared_s3_size_is_bounded_before_body_collection() {
        assert!(declared_size_is_allowed(None));
        assert!(declared_size_is_allowed(Some(1)));
        assert!(declared_size_is_allowed(Some(MAX_S3_RESPONSE_BYTES)));
        assert!(!declared_size_is_allowed(Some(MAX_S3_RESPONSE_BYTES + 1)));
        assert!(!declared_size_is_allowed(Some(-1)));
    }

    #[tokio::test]
    async fn source_hash_mismatch_is_rejected() {
        let task = valid_judge_task();
        let memory = Arc::new(MemorySource {
            objects: HashMap::from([(
                ("sources".to_owned(), task.source_object_key.clone()),
                Bytes::from_static(b"tampered"),
            )]),
            reads: Mutex::default(),
        });
        let cache = std::env::temp_dir().join(format!("project-balloon-worker-{}", Uuid::new_v4()));
        let manager = ArtifactManager::new(
            memory,
            cache.clone(),
            "problems".to_owned(),
            "sources".to_owned(),
            1024,
        );
        let error = manager.prepare(&task).await.expect_err("tampered source must fail");
        assert!(matches!(error, ArtifactError::HashMismatch { kind: "source", .. }));
        if tokio::fs::try_exists(&cache).await.expect("cache existence") {
            tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
        }
    }
}
