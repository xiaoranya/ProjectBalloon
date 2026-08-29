use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use object_store::{
    Attribute, Attributes, ObjectStore, ObjectStoreExt, PutMultipartOptions, PutOptions,
    PutPayload, WriteMultipart,
    aws::{AmazonS3, AmazonS3Builder},
    list::{PaginatedListOptions, PaginatedListStore},
    path::Path,
};
use thiserror::Error;
use tokio::{io::AsyncReadExt, time::timeout};

#[derive(Debug, Error)]
pub enum ObjectStorageError {
    #[error("object storage request timed out")]
    Timeout,
    #[error("object storage request failed: {0}")]
    Request(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectStorageObject {
    pub key: String,
    pub last_modified: Option<std::time::SystemTime>,
}

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn check_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError>;

    /// Lists objects in a bucket. Adapters that do not support enumeration
    /// keep the safe default so cleanup remains deletion-only.
    async fn list_objects(
        &self,
        _bucket: &str,
        _continuation_token: Option<&str>,
    ) -> Result<ObjectStoragePage, ObjectStorageError> {
        Err(ObjectStorageError::Request("object listing is not supported by this adapter".into()))
    }

    async fn create_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
        Err(ObjectStorageError::Request(
            "bucket creation is not supported by this object storage adapter".into(),
        ))
    }

    async fn put(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<&str>,
        content: Bytes,
    ) -> Result<(), ObjectStorageError>;

    async fn put_file(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<&str>,
        path: &std::path::Path,
    ) -> Result<(), ObjectStorageError> {
        let bytes = tokio::fs::read(path)
            .await
            .map(Bytes::from)
            .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
        self.put(bucket, key, content_type, bytes).await
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError>;

    async fn get_limited(
        &self,
        bucket: &str,
        key: &str,
        max_bytes: usize,
    ) -> Result<Bytes, ObjectStorageError> {
        let content = self.get(bucket, key).await?;
        if content.len() > max_bytes {
            return Err(ObjectStorageError::Request(format!(
                "object exceeds the {max_bytes} byte read limit"
            )));
        }
        Ok(content)
    }

    async fn get_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectStorageStream, ObjectStorageError> {
        let bytes = self.get(bucket, key).await?;
        Ok(Box::pin(futures_util::stream::once(async move { Ok(bytes) })))
    }

    async fn get_stream_limited(
        &self,
        bucket: &str,
        key: &str,
        max_bytes: usize,
    ) -> Result<ObjectStorageStream, ObjectStorageError> {
        let stream = self.get_stream(bucket, key).await?;
        let capped = futures_util::stream::unfold(
            (stream, 0_usize, false),
            move |(mut stream, total, failed)| async move {
                if failed {
                    return None;
                }
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        let next_total = total.saturating_add(chunk.len());
                        if next_total > max_bytes {
                            Some((
                                Err(ObjectStorageError::Request(format!(
                                    "object exceeds the {max_bytes} byte stream limit"
                                ))),
                                (stream, next_total, true),
                            ))
                        } else {
                            Some((Ok(chunk), (stream, next_total, false)))
                        }
                    }
                    Some(Err(error)) => Some((Err(error), (stream, total, true))),
                    None => None,
                }
            },
        );
        Ok(Box::pin(capped))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectStoragePage {
    pub objects: Vec<ObjectStorageObject>,
    pub next_continuation_token: Option<String>,
}

pub type ObjectStorageStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, ObjectStorageError>> + Send + 'static>>;

const MAX_BUFFERED_OBJECT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct ObjectStorageHandle {
    backend: Arc<dyn ObjectStorage>,
    problem_bucket: String,
    source_bucket: String,
}

impl ObjectStorageHandle {
    #[must_use]
    pub fn new(backend: Arc<dyn ObjectStorage>, problem_bucket: String) -> Self {
        Self { backend, source_bucket: problem_bucket.clone(), problem_bucket }
    }

    #[must_use]
    pub fn with_buckets(
        backend: Arc<dyn ObjectStorage>,
        problem_bucket: String,
        source_bucket: String,
    ) -> Self {
        Self { backend, problem_bucket, source_bucket }
    }

    #[must_use]
    pub fn backend(&self) -> &Arc<dyn ObjectStorage> {
        &self.backend
    }

    #[must_use]
    pub fn problem_bucket(&self) -> &str {
        &self.problem_bucket
    }

    #[must_use]
    pub fn source_bucket(&self) -> &str {
        &self.source_bucket
    }

    pub async fn check(&self) -> Result<(), ObjectStorageError> {
        self.backend.check_bucket(&self.problem_bucket).await?;
        if self.source_bucket != self.problem_bucket {
            self.backend.check_bucket(&self.source_bucket).await?;
        }
        Ok(())
    }

    pub async fn ensure_buckets(&self) -> Result<(), ObjectStorageError> {
        // object_store intentionally exposes object operations, not bucket
        // administration. Buckets are provisioned by RustFS/deployment tooling.
        self.check().await
    }
}

pub struct S3ObjectStorage {
    endpoint: String,
    region: String,
    access_key: String,
    secret_key: String,
    force_path_style: bool,
    request_timeout: Duration,
    stores: RwLock<HashMap<String, Arc<AmazonS3>>>,
}

impl S3ObjectStorage {
    pub fn new(config: S3ObjectStorageConfig) -> Result<Self, ObjectStorageError> {
        Ok(Self {
            endpoint: config.endpoint,
            region: config.region,
            access_key: config.access_key,
            secret_key: config.secret_key,
            force_path_style: config.force_path_style,
            request_timeout: config.request_timeout,
            stores: RwLock::new(HashMap::new()),
        })
    }

    fn store(&self, bucket: &str) -> Result<Arc<AmazonS3>, ObjectStorageError> {
        {
            let stores = self.stores.read().map_err(|_| {
                ObjectStorageError::Request("object storage cache is poisoned".into())
            })?;
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
                .with_virtual_hosted_style_request(!self.force_path_style)
                .build()
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?,
        );
        let mut stores = self
            .stores
            .write()
            .map_err(|_| ObjectStorageError::Request("object storage cache is poisoned".into()))?;
        if let Some(existing) = stores.get(bucket) {
            return Ok(Arc::clone(existing));
        }
        stores.insert(bucket.to_owned(), Arc::clone(&store));
        Ok(store)
    }

    fn path(key: &str) -> Result<Path, ObjectStorageError> {
        Path::parse(key).map_err(|error| ObjectStorageError::Request(error.to_string()))
    }

    fn put_attributes(content_type: Option<&str>) -> Attributes {
        content_type.map_or_else(Attributes::default, |content_type| {
            Attributes::from_iter([(Attribute::ContentType, content_type.to_owned())])
        })
    }

    fn put_options(content_type: Option<&str>) -> PutOptions {
        Self::put_attributes(content_type).into()
    }

    async fn within_timeout<T>(
        &self,
        request: impl Future<Output = Result<T, ObjectStorageError>>,
    ) -> Result<T, ObjectStorageError> {
        timeout(self.request_timeout, request).await.map_err(|_| ObjectStorageError::Timeout)?
    }

    async fn get_limited_inner(
        &self,
        bucket: &str,
        key: &str,
        max_bytes: usize,
    ) -> Result<Bytes, ObjectStorageError> {
        let store = self.store(bucket)?;
        let path = Self::path(key)?;
        let max_bytes_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        self.within_timeout(async {
            let metadata = store
                .head(&path)
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            if metadata.size > max_bytes_u64 {
                return Err(ObjectStorageError::Request(format!(
                    "object exceeds the {max_bytes} byte read limit"
                )));
            }
            let mut stream = store
                .get(&path)
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?
                .into_stream();
            let mut content = BytesMut::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ObjectStorageError::Request(error.to_string()))?;
                if content.len().saturating_add(chunk.len()) > max_bytes {
                    return Err(ObjectStorageError::Request(format!(
                        "object exceeds the {max_bytes} byte read limit"
                    )));
                }
                content.extend_from_slice(&chunk);
            }
            Ok(content.freeze())
        })
        .await
    }
}

pub struct S3ObjectStorageConfig {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub force_path_style: bool,
    pub request_timeout: Duration,
}

#[async_trait]
impl ObjectStorage for S3ObjectStorage {
    async fn check_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError> {
        let store = self.store(bucket)?;
        self.within_timeout(async {
            store
                .list_paginated(
                    None,
                    PaginatedListOptions { max_keys: Some(1), ..Default::default() },
                )
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }

    async fn list_objects(
        &self,
        bucket: &str,
        continuation_token: Option<&str>,
    ) -> Result<ObjectStoragePage, ObjectStorageError> {
        let store = self.store(bucket)?;
        self.within_timeout(async {
            let response = store
                .list_paginated(
                    None,
                    PaginatedListOptions {
                        page_token: continuation_token.map(str::to_owned),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            let objects = response
                .result
                .objects
                .into_iter()
                .map(|object| ObjectStorageObject {
                    key: object.location.to_string(),
                    last_modified: system_time_from_unix_seconds(object.last_modified.timestamp()),
                })
                .collect();
            Ok(ObjectStoragePage { objects, next_continuation_token: response.page_token })
        })
        .await
    }

    async fn put(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<&str>,
        content: Bytes,
    ) -> Result<(), ObjectStorageError> {
        let store = self.store(bucket)?;
        let path = Self::path(key)?;
        self.within_timeout(async {
            store
                .put_opts(&path, PutPayload::from_bytes(content), Self::put_options(content_type))
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }

    async fn put_file(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<&str>,
        path: &std::path::Path,
    ) -> Result<(), ObjectStorageError> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
        let store = self.store(bucket)?;
        let object_path = Self::path(key)?;
        self.within_timeout(async {
            let upload = store
                .put_multipart_opts(
                    &object_path,
                    PutMultipartOptions::from(Self::put_options(content_type).attributes),
                )
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            let mut writer = WriteMultipart::new(upload);
            let mut buffer = vec![0_u8; 5 * 1024 * 1024];
            loop {
                let read = match file.read(&mut buffer).await {
                    Ok(read) => read,
                    Err(error) => {
                        let _ = writer.abort().await;
                        return Err(ObjectStorageError::Request(error.to_string()));
                    }
                };
                if read == 0 {
                    break;
                }
                writer.put(Bytes::copy_from_slice(&buffer[..read]));
                if let Err(error) = writer.wait_for_capacity(4).await {
                    let _ = writer.abort().await;
                    return Err(ObjectStorageError::Request(error.to_string()));
                }
            }
            writer
                .finish()
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.get_limited_inner(bucket, key, MAX_BUFFERED_OBJECT_BYTES).await
    }

    async fn get_limited(
        &self,
        bucket: &str,
        key: &str,
        max_bytes: usize,
    ) -> Result<Bytes, ObjectStorageError> {
        self.get_limited_inner(bucket, key, max_bytes).await
    }

    async fn get_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectStorageStream, ObjectStorageError> {
        let store = self.store(bucket)?;
        let path = Self::path(key)?;
        let response = self
            .within_timeout(async {
                store
                    .get(&path)
                    .await
                    .map_err(|error| ObjectStorageError::Request(error.to_string()))
            })
            .await?;
        let stream = response
            .into_stream()
            .map(|chunk| chunk.map_err(|error| ObjectStorageError::Request(error.to_string())));
        Ok(Box::pin(stream))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        let store = self.store(bucket)?;
        let path = Self::path(key)?;
        self.within_timeout(async {
            store
                .delete(&path)
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }
}

fn system_time_from_unix_seconds(seconds: i64) -> Option<std::time::SystemTime> {
    if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(seconds).ok()?))
    } else {
        std::time::UNIX_EPOCH
            .checked_sub(Duration::from_secs(u64::try_from(seconds.checked_neg()?).ok()?))
    }
}

pub mod keys {
    use uuid::Uuid;

    #[must_use]
    pub fn problem_attachment(problem_id: i64, sha256: &str, filename: &str) -> String {
        format!("problems/{problem_id}/attachments/{sha256}/{}-{filename}", Uuid::new_v4())
    }

    #[must_use]
    pub fn testdata(problem_id: i64, version: i32) -> String {
        format!("problems/{problem_id}/testdata/v{version}/{}.zip", Uuid::new_v4())
    }

    #[must_use]
    pub fn interactor(problem_id: i64) -> String {
        format!("problems/{problem_id}/interactors/{}", Uuid::new_v4())
    }

    #[must_use]
    pub fn submission_source(contest_id: i64, team_id: i64, extension: &str) -> String {
        format!("submissions/{contest_id}/{team_id}/{}{extension}", Uuid::new_v4())
    }

    #[must_use]
    pub fn practice_submission_source(user_id: i64, extension: &str) -> String {
        format!("practice-submissions/{user_id}/{}{extension}", Uuid::new_v4())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::{StreamExt, TryStreamExt};

    use crate::object_storage::{ObjectStorage, ObjectStorageError, keys};

    struct BufferedStorage;

    #[async_trait]
    impl ObjectStorage for BufferedStorage {
        async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }

        async fn put(
            &self,
            _bucket: &str,
            _key: &str,
            _content_type: Option<&str>,
            _content: Bytes,
        ) -> Result<(), ObjectStorageError> {
            Ok(())
        }

        async fn get(&self, _bucket: &str, _key: &str) -> Result<Bytes, ObjectStorageError> {
            Ok(Bytes::from_static(b"streamed"))
        }

        async fn delete(&self, _bucket: &str, _key: &str) -> Result<(), ObjectStorageError> {
            Ok(())
        }
    }

    #[test]
    fn keys_are_namespaced_and_testdata_is_versioned() {
        let attachment = keys::problem_attachment(7, &"a".repeat(64), "statement.pdf");
        assert!(attachment.starts_with(&format!("problems/7/attachments/{}/", "a".repeat(64))));
        assert!(attachment.ends_with("-statement.pdf"));
        let testdata = keys::testdata(7, 3);
        assert!(testdata.starts_with("problems/7/testdata/v3/"));
        assert!(testdata.ends_with(".zip"));
        let source = keys::submission_source(3, 9, ".cpp");
        assert!(source.starts_with("submissions/3/9/"));
        assert!(source.ends_with(".cpp"));
    }

    #[tokio::test]
    async fn buffered_adapters_get_a_safe_default_stream() {
        let chunks = BufferedStorage
            .get_stream("bucket", "key")
            .await
            .expect("create object stream")
            .try_collect::<Vec<_>>()
            .await
            .expect("read object stream");
        assert_eq!(chunks, vec![Bytes::from_static(b"streamed")]);
    }

    #[tokio::test]
    async fn buffered_reads_and_streams_enforce_limits() {
        assert!(BufferedStorage.get_limited("bucket", "key", 7).await.is_err());
        let mut stream = BufferedStorage
            .get_stream_limited("bucket", "key", 8)
            .await
            .expect("create capped object stream");
        assert!(stream.next().await.expect("first capped chunk").is_ok());
        assert!(stream.next().await.is_none());

        let mut stream = BufferedStorage
            .get_stream_limited("bucket", "key", 6)
            .await
            .expect("create rejecting object stream");
        assert!(stream.next().await.expect("oversized chunk").is_err());
        assert!(stream.next().await.is_none());
    }
}
