use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use object_store::{
    ObjectStoreExt,
    aws::{AmazonS3, AmazonS3Builder},
    list::{PaginatedListOptions, PaginatedListStore},
    path::Path,
};
use project_balloon_contracts::JudgeTask;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{io::AsyncReadExt, sync::Mutex as AsyncMutex, time::timeout};
use tracing::{info, warn};
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
    endpoint: String,
    region: String,
    access_key: String,
    secret_key: String,
    request_timeout: Duration,
    stores: RwLock<HashMap<String, Arc<AmazonS3>>>,
}

const MAX_S3_RESPONSE_BYTES: u64 = 512 * 1024 * 1024;

fn declared_size_is_allowed(length: Option<u64>) -> bool {
    length.is_none_or(|length| length <= MAX_S3_RESPONSE_BYTES)
}

pub struct S3ArtifactSourceConfig {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub request_timeout: Duration,
}

impl S3ArtifactSource {
    pub fn new(config: S3ArtifactSourceConfig) -> Result<Self, ArtifactError> {
        Ok(Self {
            endpoint: config.endpoint,
            region: config.region,
            access_key: config.access_key,
            secret_key: config.secret_key,
            request_timeout: config.request_timeout,
            stores: RwLock::new(HashMap::new()),
        })
    }

    fn store(&self, bucket: &str) -> Result<Arc<AmazonS3>, ArtifactError> {
        {
            let stores = self
                .stores
                .read()
                .map_err(|_| ArtifactError::Request("object storage cache is poisoned".into()))?;
            if let Some(store) = stores.get(bucket) {
                return Ok(Arc::clone(store));
            }
        }

        let store = Arc::new(
            AmazonS3Builder::new()
                .with_endpoint(self.endpoint.clone())
                .with_region(self.region.clone())
                .with_bucket_name(bucket)
                .with_access_key_id(self.access_key.clone())
                .with_secret_access_key(self.secret_key.clone())
                .with_allow_http(self.endpoint.starts_with("http://"))
                .with_virtual_hosted_style_request(false)
                .build()
                .map_err(|error| ArtifactError::Request(error.to_string()))?,
        );
        let mut stores = self
            .stores
            .write()
            .map_err(|_| ArtifactError::Request("object storage cache is poisoned".into()))?;
        if let Some(existing) = stores.get(bucket) {
            return Ok(Arc::clone(existing));
        }
        stores.insert(bucket.to_owned(), Arc::clone(&store));
        Ok(store)
    }
}

#[async_trait]
impl ArtifactSource for S3ArtifactSource {
    async fn check_bucket(&self, bucket: &str) -> Result<(), ArtifactError> {
        let store = self.store(bucket)?;
        timeout(
            self.request_timeout,
            store.list_paginated(
                None,
                PaginatedListOptions { max_keys: Some(1), ..Default::default() },
            ),
        )
        .await
        .map_err(|_| ArtifactError::Timeout)?
        .map(|_| ())
        .map_err(|error| ArtifactError::Request(error.to_string()))
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ArtifactError> {
        let store = self.store(bucket)?;
        let path = Path::parse(key).map_err(|error| ArtifactError::Request(error.to_string()))?;
        let metadata = timeout(self.request_timeout, store.head(&path))
            .await
            .map_err(|_| ArtifactError::Timeout)?
            .map_err(|error| ArtifactError::Request(error.to_string()))?;
        if !declared_size_is_allowed(Some(metadata.size)) {
            return Err(ArtifactError::TooLarge(MAX_S3_RESPONSE_BYTES));
        }
        let mut response = timeout(self.request_timeout, store.get(&path))
            .await
            .map_err(|_| ArtifactError::Timeout)?
            .map_err(|error| ArtifactError::Request(error.to_string()))?
            .into_stream();
        let mut body = BytesMut::new();
        loop {
            let chunk = timeout(self.request_timeout, response.next())
                .await
                .map_err(|_| ArtifactError::Timeout)?
                .transpose()
                .map_err(|error| ArtifactError::Request(error.to_string()))?;
            let Some(chunk) = chunk else { break };
            if body.len().saturating_add(chunk.len()) > MAX_S3_RESPONSE_BYTES as usize {
                return Err(ArtifactError::TooLarge(MAX_S3_RESPONSE_BYTES));
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
    testdata_cache_max_bytes: u64,
    /// Serializes testdata-cache eviction within this worker process. Cache
    /// entries themselves are safe to share across concurrent judgements; the
    /// size scan plus removal is the only read-modify-write step.
    evict_lock: Arc<AsyncMutex<()>>,
}

impl ArtifactManager {
    #[must_use]
    pub fn new(
        source: Arc<dyn ArtifactSource>,
        cache_dir: PathBuf,
        problem_bucket: String,
        source_bucket: String,
        max_artifact_bytes: u64,
        testdata_cache_max_bytes: u64,
    ) -> Self {
        Self {
            source,
            cache_dir,
            problem_bucket,
            source_bucket,
            max_artifact_bytes,
            testdata_cache_max_bytes,
            evict_lock: Arc::new(AsyncMutex::new(())),
        }
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
                Ok(()) => {
                    // Refresh the recency marker so eviction stays a true LRU:
                    // hits must outlive stale-but-newer inserts.
                    refresh_recency(&path).await;
                    return Ok(path);
                }
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
        self.evict_testdata_cache(&path).await;
        Ok(path)
    }

    /// Evicts least-recently-used cache entries (by mtime, which cache hits
    /// refresh) until the directory fits the configured cap. The entry just
    /// stored is never evicted by its own insertion; eviction trouble is
    /// logged, never fatal — missing entries are safely re-fetched.
    async fn evict_testdata_cache(&self, keep: &std::path::Path) {
        if self.testdata_cache_max_bytes == 0 {
            return;
        }
        let _eviction = self.evict_lock.lock().await;
        let mut entries: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        let mut total = 0_u64;
        let mut read_dir = match tokio::fs::read_dir(self.testdata_cache_dir()).await {
            Ok(read_dir) => read_dir,
            Err(error) => {
                warn!(error = %error, "testdata cache eviction could not list the cache directory");
                return;
            }
        };
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let Ok(metadata) = entry.metadata().await else { continue };
            if !metadata.is_file() {
                continue;
            }
            total = total.saturating_add(metadata.len());
            entries.push((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ));
        }
        entries.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)));
        for (path, size, _) in entries {
            if total <= self.testdata_cache_max_bytes {
                break;
            }
            if path == keep {
                continue;
            }
            match remove_if_present(&path).await {
                Ok(()) => {
                    total = total.saturating_sub(size);
                    info!(
                        file = %path.display(),
                        bytes = size,
                        remaining_bytes = total,
                        cap_bytes = self.testdata_cache_max_bytes,
                        "evicted testdata cache entry under LRU pressure"
                    );
                }
                Err(error) => {
                    warn!(
                        file = %path.display(),
                        error = %error,
                        "testdata cache eviction could not remove an entry; stopping this pass"
                    );
                    break;
                }
            }
        }
        if total > self.testdata_cache_max_bytes {
            warn!(
                remaining_bytes = total,
                cap_bytes = self.testdata_cache_max_bytes,
                "testdata cache remains above its size cap after eviction"
            );
        }
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

/// Best-effort mtime refresh marking a cache hit as recently used. A failed
/// refresh only degrades LRU towards FIFO; it must not fail the judgement.
async fn refresh_recency(path: &std::path::Path) {
    let refresh = tokio::task::spawn_blocking({
        let path = path.to_owned();
        move || -> std::io::Result<()> {
            std::fs::File::options()
                .write(true)
                .open(&path)?
                .set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
        }
    });
    if let Err(error) = refresh.await.expect("recency refresh task must not panic") {
        warn!(file = %path.display(), error = %error, "could not refresh testdata cache recency marker");
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
            0,
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
            0,
        );
        let error = manager.prepare(&task).await.expect_err("tampered source must fail");
        assert!(matches!(error, ArtifactError::HashMismatch { kind: "source", .. }));
        if tokio::fs::try_exists(&cache).await.expect("cache existence") {
            tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
        }
    }

    #[test]
    fn artifact_sizes_are_bounded_by_the_configured_maximum() {
        let manager = ArtifactManager::new(
            Arc::new(MemorySource { objects: HashMap::new(), reads: Mutex::default() }),
            std::env::temp_dir().join(format!("project-balloon-sizes-{}", Uuid::new_v4())),
            "problems".to_owned(),
            "sources".to_owned(),
            1024,
            0,
        );
        assert!(manager.validate_size_u64(0).is_ok());
        assert!(manager.validate_size_u64(1024).is_ok());
        assert!(matches!(manager.validate_size_u64(1025), Err(ArtifactError::TooLarge(1024))));
        assert!(manager.validate_size_u64(u64::MAX).is_err());
    }

    fn cache_manager(
        source: Arc<MemorySource>,
        cache: std::path::PathBuf,
        max_artifact_bytes: u64,
        testdata_cache_max_bytes: u64,
    ) -> ArtifactManager {
        ArtifactManager::new(
            source,
            cache,
            "problems".to_owned(),
            "sources".to_owned(),
            max_artifact_bytes,
            testdata_cache_max_bytes,
        )
    }

    fn task_with_testdata(content: &[u8], key: &str, base: &JudgeTask) -> JudgeTask {
        let mut task = base.clone();
        task.source_sha256 = hex::encode(Sha256::digest(b"int main() { return 0; }"));
        task.testdata_object_key = key.to_owned();
        task.testdata_sha256 = hex::encode(Sha256::digest(content));
        task
    }

    async fn backdate_mtime(path: &std::path::Path, seconds_ago: u64) {
        let modified = SystemTime::now() - Duration::from_secs(seconds_ago);
        tokio::task::spawn_blocking({
            let path = path.to_owned();
            move || {
                std::fs::File::options()
                    .write(true)
                    .open(&path)?
                    .set_times(std::fs::FileTimes::new().set_modified(modified))
            }
        })
        .await
        .expect("backdate task must not panic")
        .expect("backdate file");
    }

    #[tokio::test]
    async fn testdata_cache_eviction_is_lru_under_pressure() {
        let base = valid_judge_task();
        let contents: Vec<Vec<u8>> =
            vec![vec![b'a'; 100], vec![b'b'; 200], vec![b'c'; 300], vec![b'd'; 400]];
        let objects: Vec<(String, Bytes)> = contents
            .iter()
            .enumerate()
            .map(|(index, content)| {
                (format!("problems/testdata-{index}"), Bytes::from(content.clone()))
            })
            .collect();
        let source_object = ("sources".to_owned(), base.source_object_key.clone());
        let memory = Arc::new(MemorySource {
            objects: HashMap::from([(
                source_object,
                Bytes::from_static(b"int main() { return 0; }"),
            )])
            .into_iter()
            .chain(objects.into_iter().map(|(key, value)| (("problems".to_owned(), key), value)))
            .collect(),
            reads: Mutex::default(),
        });
        let cache = std::env::temp_dir().join(format!("project-balloon-lru-{}", Uuid::new_v4()));
        // The cap fits exactly the two newest entries, so pushing the fourth
        // entry in must evict the two oldest.
        let manager = cache_manager(memory.clone(), cache.clone(), 4096, 700);
        manager.preflight().await.expect("preflight");

        let mut paths = Vec::new();
        for (index, seconds_ago) in [(0_usize, 5_000_u64), (1, 4_000), (2, 3_000)] {
            let task =
                task_with_testdata(&contents[index], &format!("problems/testdata-{index}"), &base);
            let path = manager.prepare(&task).await.expect("prepare").testdata_archive;
            backdate_mtime(&path, seconds_ago).await;
            paths.push(path);
        }
        let hot_task = task_with_testdata(&contents[3], "problems/testdata-3", &base);
        let hot_path =
            manager.prepare(&hot_task).await.expect("prepare hot entry").testdata_archive;

        assert!(!paths[0].exists(), "oldest entry must be evicted");
        assert!(!paths[1].exists(), "second-oldest entry must be evicted");
        assert!(paths[2].exists(), "recent entry must survive");
        assert!(hot_path.exists(), "the just-stored entry must survive");
        tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
    }

    #[tokio::test]
    async fn cache_hits_refresh_recency_and_survive_pressure() {
        let base = valid_judge_task();
        let contents: Vec<Vec<u8>> = vec![vec![b'a'; 100], vec![b'b'; 200], vec![b'c'; 300]];
        let memory = Arc::new(MemorySource {
            objects: HashMap::from([
                (
                    ("sources".to_owned(), base.source_object_key.clone()),
                    Bytes::from_static(b"int main() { return 0; }"),
                ),
                (
                    ("problems".to_owned(), "problems/t0".to_owned()),
                    Bytes::from(contents[0].clone()),
                ),
                (
                    ("problems".to_owned(), "problems/t1".to_owned()),
                    Bytes::from(contents[1].clone()),
                ),
                (
                    ("problems".to_owned(), "problems/t2".to_owned()),
                    Bytes::from(contents[2].clone()),
                ),
            ]),
            reads: Mutex::default(),
        });
        let cache =
            std::env::temp_dir().join(format!("project-balloon-lru-hit-{}", Uuid::new_v4()));
        // The cap is one byte below the total of all three entries, so the
        // insert of the third must evict exactly the least-recently-used one.
        let manager = cache_manager(memory, cache.clone(), 4096, 599);
        manager.preflight().await.expect("preflight");

        let old_task = task_with_testdata(&contents[0], "problems/t0", &base);
        let old_path = manager.prepare(&old_task).await.expect("prepare old").testdata_archive;
        backdate_mtime(&old_path, 5_000).await;
        let older_task = task_with_testdata(&contents[1], "problems/t1", &base);
        let older_path = manager.prepare(&older_task).await.expect("prepare").testdata_archive;
        backdate_mtime(&older_path, 4_000).await;

        // A cache hit refreshes recency: despite the oldest insert time, the
        // hit entry must now be newer than the untouched middle entry.
        manager.prepare(&old_task).await.expect("cache hit");
        let new_task = task_with_testdata(&contents[2], "problems/t2", &base);
        let new_path = manager.prepare(&new_task).await.expect("prepare new").testdata_archive;

        assert!(old_path.exists(), "the refreshed hit must survive eviction");
        assert!(!older_path.exists(), "the untouched entry must be evicted instead");
        assert!(new_path.exists(), "the just-stored entry must survive");
        tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
    }

    #[tokio::test]
    async fn an_oversized_entry_is_kept_even_when_it_alone_exceeds_the_cap() {
        let base = valid_judge_task();
        let content = vec![b'x'; 500];
        let memory = Arc::new(MemorySource {
            objects: HashMap::from([
                (
                    ("sources".to_owned(), base.source_object_key.clone()),
                    Bytes::from_static(b"int main() { return 0; }"),
                ),
                (("problems".to_owned(), "problems/big".to_owned()), Bytes::from(content.clone())),
            ]),
            reads: Mutex::default(),
        });
        let cache =
            std::env::temp_dir().join(format!("project-balloon-lru-cap-{}", Uuid::new_v4()));
        let manager = cache_manager(memory, cache.clone(), 4096, 10);
        manager.preflight().await.expect("preflight");

        let task = task_with_testdata(&content, "problems/big", &base);
        let path = manager.prepare(&task).await.expect("prepare oversized entry").testdata_archive;
        assert!(path.exists(), "the only entry must never be evicted by its own insert");
        tokio::fs::remove_dir_all(cache).await.expect("remove test cache");
    }
}
