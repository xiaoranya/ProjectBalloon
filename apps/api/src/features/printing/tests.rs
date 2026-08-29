use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::Bytes;
use sqlx::PgPool;

use crate::features::printing::model::{CreateRequest, estimate_pages};
use crate::features::printing::service::PrintingService;
use crate::{
    features::auth::model::{AuthUser, UserType},
    object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
};

#[derive(Default)]
struct MemoryStorage {
    objects: Mutex<HashMap<(String, String), Bytes>>,
}

#[async_trait]
impl ObjectStorage for MemoryStorage {
    async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
        Ok(())
    }
    async fn put(
        &self,
        bucket: &str,
        key: &str,
        _content_type: Option<&str>,
        content: Bytes,
    ) -> Result<(), ObjectStorageError> {
        self.objects.lock().expect("storage lock").insert((bucket.into(), key.into()), content);
        Ok(())
    }
    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.objects
            .lock()
            .expect("storage lock")
            .get(&(bucket.into(), key.into()))
            .cloned()
            .ok_or_else(|| ObjectStorageError::Request("not found".into()))
    }
    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        self.objects.lock().expect("storage lock").remove(&(bucket.into(), key.into()));
        Ok(())
    }
}

#[test]
fn print_content_size_controls_and_page_estimate_are_closed() {
    assert_eq!(estimate_pages("hello"), 1);
    assert_eq!(estimate_pages(&"line\n".repeat(50)), 2);
    assert!(CreateRequest { content: "ok".into() }.validate().is_ok());
    assert!(CreateRequest { content: "\0".into() }.validate().is_err());
    assert!(CreateRequest { content: "x".repeat(20 * 1024 + 1) }.validate().is_err());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL and the cupsfilter executable"]
async fn print_request_renders_archives_limits_and_hides_other_teams(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-root', 'hash', 'Print Root', 'SUPER_ADMIN') RETURNING id")
        .fetch_one(&pool).await.expect("insert print administrator");
    let team_user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-team', 'hash', 'Print Team', 'TEAM') RETURNING id")
        .fetch_one(&pool).await.expect("insert print team user");
    let other_user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('print-other', 'hash', 'Print Other', 'TEAM') RETURNING id")
        .fetch_one(&pool).await.expect("insert other print user");
    let team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name, seat_no) VALUES ('Print Team', 'A01') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert print team");
    let other_team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name) VALUES ('Print Other') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert other print team");
    for (user, team) in [(team_user_id, team_id), (other_user_id, other_team_id)] {
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(user)
            .bind(team)
            .execute(&pool)
            .await
            .expect("link print team");
    }
    let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, end_at) VALUES ('Print Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour') RETURNING id")
        .fetch_one(&pool).await.expect("insert print contest");
    for team in [team_id, other_team_id] {
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id).bind(team).execute(&pool).await.expect("roster print team");
    }
    let team = AuthUser {
        id: team_user_id,
        username: "print-team".into(),
        display_name: "Print Team".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let other = AuthUser {
        id: other_user_id,
        username: "print-other".into(),
        display_name: "Print Other".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let admin = AuthUser {
        id: admin_id,
        username: "print-root".into(),
        display_name: "Print Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let storage = ObjectStorageHandle::with_buckets(
        Arc::new(MemoryStorage::default()),
        "problems".into(),
        "artifacts".into(),
    );
    let service = PrintingService::new(pool.clone());
    let created = service
        .create(
            contest_id,
            CreateRequest { content: "hello printer\n".into() }.validate().expect("valid print"),
            &team,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("create print request");
    assert_eq!(created.status, "QUEUED");
    assert_eq!(created.page_count, 1);
    let pdf = service.pdf(created.id, &team, &storage).await.expect("download own PDF");
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(service.pdf(created.id, &other, &storage).await.is_err());
    assert!(
        service
            .create(
                contest_id,
                CreateRequest { content: "again".into() }.validate().expect("valid second print"),
                &team,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage
            )
            .await
            .is_err()
    );
    let rejected = service
        .transition(
            created.id,
            "REJECT",
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            Some("Not printable".into()),
        )
        .await
        .expect("reject print");
    assert_eq!(rejected.status, "REJECTED");
    assert_eq!(rejected.failed_reason.as_deref(), Some("Not printable"));
    assert!(service.list_mine(contest_id, &other).await.expect("list other print jobs").is_empty());
}
