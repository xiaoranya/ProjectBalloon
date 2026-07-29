use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use aws_sdk_s3::{
    Client,
    config::{Credentials, Region},
    primitives::ByteStream,
};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
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

    async fn get_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectStorageStream, ObjectStorageError> {
        let bytes = self.get(bucket, key).await?;
        Ok(Box::pin(futures_util::stream::once(async move { Ok(bytes) })))
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
    client: Client,
    request_timeout: Duration,
}

impl S3ObjectStorage {
    #[must_use]
    pub fn new(config: S3ObjectStorageConfig) -> Self {
        let credentials = Credentials::new(
            config.access_key,
            config.secret_key,
            None,
            None,
            "project-balloon-static",
        );
        let sdk_config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(Region::new(config.region))
            .credentials_provider(credentials)
            .endpoint_url(config.endpoint)
            .force_path_style(config.force_path_style)
            .build();
        Self { client: Client::from_conf(sdk_config), request_timeout: config.request_timeout }
    }

    async fn within_timeout<T>(
        &self,
        request: impl Future<Output = Result<T, ObjectStorageError>>,
    ) -> Result<T, ObjectStorageError> {
        timeout(self.request_timeout, request).await.map_err(|_| ObjectStorageError::Timeout)?
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
        self.within_timeout(async {
            self.client
                .head_bucket()
                .bucket(bucket)
                .send()
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
        self.within_timeout(async {
            let mut request = self.client.list_objects_v2().bucket(bucket);
            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }
            let response = request
                .send()
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            let objects = response
                .contents()
                .iter()
                .filter_map(|object| {
                    object.key().map(|key| ObjectStorageObject {
                        key: key.to_owned(),
                        last_modified: object.last_modified().and_then(|date| {
                            std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(
                                u64::try_from(date.secs()).ok()?,
                            ))
                        }),
                    })
                })
                .collect();
            Ok(ObjectStoragePage {
                objects,
                next_continuation_token: response.next_continuation_token().map(str::to_owned),
            })
        })
        .await
    }

    async fn create_bucket(&self, bucket: &str) -> Result<(), ObjectStorageError> {
        self.within_timeout(async {
            self.client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
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
        self.within_timeout(async {
            let mut request =
                self.client.put_object().bucket(bucket).key(key).body(ByteStream::from(content));
            if let Some(content_type) = content_type {
                request = request.content_type(content_type);
            }
            request
                .send()
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
        let body = ByteStream::from_path(path)
            .await
            .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
        self.within_timeout(async {
            let mut request = self.client.put_object().bucket(bucket).key(key).body(body);
            if let Some(content_type) = content_type {
                request = request.content_type(content_type);
            }
            request
                .send()
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.within_timeout(async {
            let response = self
                .client
                .get_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map_err(|error| ObjectStorageError::Request(error.to_string()))?;
            response
                .body
                .collect()
                .await
                .map(|body| body.into_bytes())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
    }

    async fn get_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectStorageStream, ObjectStorageError> {
        let response = self
            .within_timeout(async {
                self.client
                    .get_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|error| ObjectStorageError::Request(error.to_string()))
            })
            .await?;
        let stream = tokio_util::io::ReaderStream::new(response.body.into_async_read())
            .map(|chunk| chunk.map_err(|error| ObjectStorageError::Request(error.to_string())));
        Ok(Box::pin(stream))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        self.within_timeout(async {
            self.client
                .delete_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map(|_| ())
                .map_err(|error| ObjectStorageError::Request(error.to_string()))
        })
        .await
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
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::TryStreamExt;

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
}
