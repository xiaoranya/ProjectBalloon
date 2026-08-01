use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use s3::{Bucket, BucketConfiguration, Region, creds::Credentials};
use thiserror::Error;
use tokio::time::timeout;

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
        self.ensure_bucket(&self.problem_bucket).await?;
        if self.source_bucket != self.problem_bucket {
            self.ensure_bucket(&self.source_bucket).await?;
        }
        Ok(())
    }

    async fn ensure_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError> {
        if self.backend.check_bucket(bucket).await.is_ok() {
            return Ok(());
        }
        let create_result = self.backend.create_bucket(bucket).await;
        match self.backend.check_bucket(bucket).await {
            Ok(()) => Ok(()),
            Err(check_error) => Err(create_result.err().unwrap_or(check_error)),
        }
    }
}

pub struct S3ObjectStorage {
    endpoint: String,
    region: String,
    credentials: Credentials,
    force_path_style: bool,
    request_timeout: Duration,
}

impl S3ObjectStorage {
    pub fn new(config: S3ObjectStorageConfig) -> Result<Self, ObjectStorageError> {
        let credentials =
            Credentials::new(Some(&config.access_key), Some(&config.secret_key), None, None, None)
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
        Ok(Self {
            endpoint: config.endpoint,
            region: config.region,
            credentials,
            force_path_style: config.force_path_style,
            request_timeout: config.request_timeout,
        })
    }

    fn bucket(&self, name: &str) -> Result<Box<Bucket>, ObjectStorageError> {
        let region =
            Region::Custom { region: self.region.clone(), endpoint: self.endpoint.clone() };
        let mut bucket = Bucket::new(name, region, self.credentials.clone())
            .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
        if self.force_path_style {
            bucket = bucket.with_path_style();
        }
        Ok(bucket)
    }

    fn region(&self) -> Region {
        Region::Custom { region: self.region.clone(), endpoint: self.endpoint.clone() }
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
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            let (metadata, _) = bucket
                .head_object(key)
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            if metadata.content_length.is_some_and(|length| {
                length < 0
                    || u64::try_from(length).ok().is_some_and(|length| length > max_bytes as u64)
            }) {
                return Err(ObjectStorageError::Request(format!(
                    "object exceeds the {max_bytes} byte read limit"
                )));
            }
            let mut stream = bucket
                .get_object_stream(key)
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            let mut content = BytesMut::new();
            while let Some(chunk) = stream.bytes().next().await {
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
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            bucket
                .list_page(String::new(), None, None, None, Some(1))
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
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            let (response, _) = bucket
                .list_page(String::new(), None, continuation_token.map(str::to_owned), None, None)
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            let objects = response
                .contents
                .into_iter()
                .map(|object| ObjectStorageObject {
                    key: object.key,
                    last_modified: parse_last_modified(&object.last_modified),
                })
                .collect();
            Ok(ObjectStoragePage {
                objects,
                next_continuation_token: response.next_continuation_token,
            })
        })
        .await
    }

    async fn create_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError> {
        let region = self.region();
        let credentials = self.credentials.clone();
        self.within_timeout(async {
            let result = if self.force_path_style {
                Bucket::create_with_path_style(
                    bucket,
                    region,
                    credentials,
                    BucketConfiguration::default(),
                )
                .await
            } else {
                Bucket::create(bucket, region, credentials, BucketConfiguration::default()).await
            };
            result.map(|_| ()).map_err(|error| ObjectStorageError::Request(error.to_string()))
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
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            let result = if let Some(content_type) = content_type {
                bucket.put_object_with_content_type(key, content.as_ref(), content_type).await
            } else {
                bucket.put_object(key, content.as_ref()).await
            };
            result.map(|_| ()).map_err(|error| ObjectStorageError::Request(error.to_string()))
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
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            let result = if let Some(content_type) = content_type {
                bucket.put_object_stream_with_content_type(&mut file, key, content_type).await
            } else {
                bucket.put_object_stream(&mut file, key).await
            };
            result.map(|_| ()).map_err(|error| ObjectStorageError::Request(error.to_string()))
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
        let bucket = self.bucket(bucket)?;
        let response = self
            .within_timeout(async {
                bucket
                    .get_object_stream(key)
                    .await
                    .map_err(|error| ObjectStorageError::Request(error.to_string()))
            })
            .await?;
        let stream = futures_util::stream::unfold(response, |mut response| async move {
            response.bytes().next().await.map(|chunk| {
                (chunk.map_err(|error| ObjectStorageError::Request(error.to_string())), response)
            })
        });
        Ok(Box::pin(stream))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        let bucket = self.bucket(bucket)?;
        self.within_timeout(async {
            bucket
                .delete_object(key)
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }
}

fn parse_last_modified(value: &str) -> Option<std::time::SystemTime> {
    let timestamp =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()?;
    let seconds = timestamp.unix_timestamp();
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

    use super::{ObjectStorage, ObjectStorageError, keys};

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
