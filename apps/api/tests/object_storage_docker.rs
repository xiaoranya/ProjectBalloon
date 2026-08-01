use std::{env, time::Duration};

use bytes::Bytes;
use project_balloon_api::object_storage::{
    ObjectStorageHandle, S3ObjectStorage, S3ObjectStorageConfig,
};
use uuid::Uuid;

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this integration test"))
}

#[tokio::test]
#[ignore = "requires an S3-compatible service configured through PROJECT_BALLOON_TEST_S3_* variables"]
async fn rustfs_put_get_delete_round_trip() {
    let bucket = env::var("PROJECT_BALLOON_TEST_S3_BUCKET")
        .unwrap_or_else(|_| "project-balloon-integration".to_owned());
    let storage = ObjectStorageHandle::new(
        std::sync::Arc::new(
            S3ObjectStorage::new(S3ObjectStorageConfig {
                endpoint: required_env("PROJECT_BALLOON_TEST_S3_ENDPOINT"),
                region: env::var("PROJECT_BALLOON_TEST_S3_REGION")
                    .unwrap_or_else(|_| "us-east-1".to_owned()),
                access_key: required_env("PROJECT_BALLOON_TEST_S3_ACCESS_KEY"),
                secret_key: required_env("PROJECT_BALLOON_TEST_S3_SECRET_KEY"),
                force_path_style: true,
                request_timeout: Duration::from_secs(5),
            })
            .expect("object storage credentials must be valid"),
        ),
        bucket.clone(),
    );
    storage.ensure_buckets().await.expect("integration bucket must be available");

    let key = format!("integration-tests/{}.txt", Uuid::new_v4());
    let expected = Bytes::from_static(b"project-balloon-rustfs-round-trip");
    storage
        .backend()
        .put(&bucket, &key, Some("text/plain"), expected.clone())
        .await
        .expect("object upload must succeed");
    let actual =
        storage.backend().get(&bucket, &key).await.expect("uploaded object must be readable");
    assert_eq!(actual, expected);

    storage.backend().delete(&bucket, &key).await.expect("object deletion must succeed");
    assert!(storage.backend().get(&bucket, &key).await.is_err());
}
