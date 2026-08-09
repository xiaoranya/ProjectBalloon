use std::{
    collections::HashMap,
    io::{Cursor, Write},
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use sqlx::PgPool;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::ProblemService;
use crate::{
    features::auth::model::{AuthUser, UserType},
    features::problems::model::{
        AttachmentKind, ProblemListQuery, ValidatedProblem, ValidatedProblemUpdate,
        ValidatedStatement,
    },
    object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
    object_storage_cleanup::{ObjectStorageCleanupConfig, ObjectStorageCleanupRunner},
};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn contest_manager_catalog_requires_an_assigned_contest_scope(pool: PgPool) {
    let creator_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-owner', 'test-hash', 'Owner', 'SUPER_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert catalog owner");
    let admin_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-admin', 'test-hash', 'Contest Admin', 'STAFF', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest admin");
    let managed_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Managed Catalog', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert managed contest");
    let other_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Other Catalog', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert other contest");
    sqlx::query("INSERT INTO contest_management_assignments (user_id, contest_id) VALUES ($1, $2)")
        .bind(admin_id)
        .bind(managed_contest_id)
        .execute(&pool)
        .await
        .expect("assign contest scope");
    let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('catalog-problem', 'Catalog Problem', $1) RETURNING id",
        )
        .bind(creator_id)
        .fetch_one(&pool)
        .await
        .expect("insert catalog problem");
    sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(managed_contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem to managed contest");
    let actor = AuthUser {
        id: admin_id,
        username: "catalog-admin".into(),
        display_name: "Contest Admin".into(),
        user_type: UserType::Staff,
        permissions: vec!["CONTEST_MANAGE".into()],
        password_reset_required: false,
    };
    let service = ProblemService::new(pool.clone());

    let page = service
        .list(ProblemListQuery { page: 0, size: 100, contest_id: Some(managed_contest_id) }, &actor)
        .await
        .expect("assigned contest scope can read shared problem metadata");
    assert_eq!(page.total_elements, 1);
    assert_eq!(page.content[0].slug, "catalog-problem");
    let detail = service
        .get(problem_id, &actor)
        .await
        .expect("fully scoped contest admin can read problem detail");
    let updated = service
        .update(
            problem_id,
            ValidatedProblemUpdate {
                expected_version: detail.version,
                slug: None,
                title: Some("Managed Problem".into()),
                time_limit_ms: None,
                memory_limit_mb: None,
                output_limit_kb: None,
                languages_json: None,
                default_lang_code: None,
                judge_mode: None,
                interactor_object_key: None,
                interactor_sha256: None,
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("fully scoped contest admin can update problem metadata");
    assert_eq!(updated.title, "Managed Problem");
    service
        .upsert_statement(
            problem_id,
            ValidatedStatement { lang_code: "en".into(), body: "# Managed".into() },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("fully scoped contest admin can update statements");

    assert!(
        service
            .list(ProblemListQuery { page: 0, size: 100, contest_id: None }, &actor)
            .await
            .is_err()
    );
    assert!(
        service
            .list(
                ProblemListQuery { page: 0, size: 100, contest_id: Some(other_contest_id) },
                &actor,
            )
            .await
            .is_err()
    );
    sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'B', 1)",
        )
        .bind(other_contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("share problem with foreign contest");
    assert!(service.get(problem_id, &actor).await.is_err());
    assert!(
        service
            .upsert_statement(
                problem_id,
                ValidatedStatement { lang_code: "en".into(), body: "# Forbidden".into() },
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn super_admin_can_create_and_delete_an_unassigned_problem(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('catalog-super', 'test-hash', 'Super Admin', 'SUPER_ADMIN', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert super admin");
    let actor = AuthUser {
        id: user_id,
        username: "catalog-super".into(),
        display_name: "Super Admin".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let service = ProblemService::new(pool.clone());
    let created = service
        .create(
            ValidatedProblem {
                slug: "created-problem".into(),
                title: "Created Problem".into(),
                time_limit_ms: 1_000,
                memory_limit_mb: 256,
                output_limit_kb: 65_536,
                languages_json: "[\"cpp\"]".into(),
                default_lang_code: "en".into(),
                judge_mode: "STANDARD".into(),
                interactor_object_key: None,
                interactor_sha256: None,
            },
            actor.id,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("super admin can create a problem");
    assert_eq!(created.slug, "created-problem");
    service
        .delete(created.id, actor.id, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("super admin can delete an unassigned problem");
    let deleted_at = sqlx::query_scalar::<_, Option<time::OffsetDateTime>>(
        "SELECT deleted_at FROM problems WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .expect("read soft-deleted problem");
    assert!(deleted_at.is_some());
}

#[derive(Default)]
struct MemoryStorage {
    objects: Mutex<HashMap<(String, String), Bytes>>,
    fail_delete: Mutex<bool>,
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
        self.objects
            .lock()
            .expect("memory storage lock")
            .insert((bucket.to_owned(), key.to_owned()), content);
        Ok(())
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.objects
            .lock()
            .expect("memory storage lock")
            .get(&(bucket.to_owned(), key.to_owned()))
            .cloned()
            .ok_or_else(|| ObjectStorageError::Request("not found".into()))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        if *self.fail_delete.lock().expect("delete failure lock") {
            return Err(ObjectStorageError::Request("temporary delete failure".into()));
        }
        self.objects
            .lock()
            .expect("memory storage lock")
            .remove(&(bucket.to_owned(), key.to_owned()));
        Ok(())
    }
}

fn testdata_zip(case_name: &str, content: &[u8]) -> Bytes {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for extension in ["in", "out"] {
        writer
            .start_file(format!("{case_name}.{extension}"), options)
            .expect("start test-data fixture entry");
        writer.write_all(content).expect("write test-data fixture entry");
    }
    Bytes::from(writer.finish().expect("finish test-data fixture").into_inner())
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn statement_persists_markdown_and_returns_safe_html(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('admin', 'test-hash', 'Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert admin");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, created_by) VALUES ('sum', 'Sum', $1) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    let actor = AuthUser {
        id: user_id,
        username: "admin".into(),
        display_name: "Admin".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let response = ProblemService::new(pool)
        .upsert_statement(
            problem_id,
            ValidatedStatement {
                lang_code: "en".into(),
                body: "# Sum\n<script>alert(1)</script>".into(),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("upsert statement");
    assert!(response.body.contains("<script>"));
    assert!(response.rendered_html.contains("<h1>Sum</h1>"));
    assert!(!response.rendered_html.contains("<script>"));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn attachment_lifecycle_keeps_database_and_object_storage_consistent(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('attachment-admin', 'test-hash', 'Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert admin");
    let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('attachment', 'Attachment', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
    let actor = AuthUser {
        id: user_id,
        username: "attachment-admin".into(),
        display_name: "Admin".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let memory = Arc::new(MemoryStorage::default());
    let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
    let content = Bytes::from_static(b"sample attachment");
    let service = ProblemService::new(pool.clone());
    let response = service
        .upload_attachment(
            problem_id,
            AttachmentKind::Sample,
            "sample.txt".into(),
            Some("text/plain".into()),
            content.clone(),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("upload attachment");
    assert_eq!(response.bytes, i64::try_from(content.len()).expect("small fixture"));
    assert_eq!(response.sha256.len(), 64);
    let object_key =
        sqlx::query_scalar::<_, String>("SELECT object_key FROM problem_attachments WHERE id = $1")
            .bind(response.id)
            .fetch_one(&pool)
            .await
            .expect("load attachment object key");
    let stored = memory.get("problems-test", &object_key).await.expect("stored object must exist");
    assert_eq!(stored, content);

    let download = service
        .download_attachment(problem_id, response.id, &actor, &storage)
        .await
        .expect("download attachment");
    assert_eq!(download.filename, "sample.txt");
    assert_eq!(download.content_type.as_deref(), Some("text/plain"));
    assert_eq!(download.content, content);

    *memory.fail_delete.lock().expect("delete failure lock") = true;
    service
        .delete_attachment(
            problem_id,
            response.id,
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("delete attachment");
    let metadata_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM problem_attachments WHERE id = $1)",
    )
    .bind(response.id)
    .fetch_one(&pool)
    .await
    .expect("check attachment metadata");
    assert!(!metadata_exists);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
            .fetch_one(&pool)
            .await
            .expect("count deferred attachment cleanup"),
        1
    );
    assert_eq!(
        memory.get("problems-test", &object_key).await.expect("orphan retained for retry"),
        content
    );
    *memory.fail_delete.lock().expect("delete failure lock") = false;
    let cleanup_runner = ObjectStorageCleanupRunner::new(
        pool.clone(),
        storage.clone(),
        ObjectStorageCleanupConfig {
            poll_interval: Duration::from_secs(1),
            lease: Duration::from_secs(30),
            retry_base: Duration::from_millis(1),
            batch_size: 10,
        },
    );
    assert_eq!(cleanup_runner.run_once().await.expect("retry attachment cleanup"), 1);
    assert!(memory.get("problems-test", &object_key).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM object_storage_cleanup_tasks")
            .fetch_one(&pool)
            .await
            .expect("count completed attachment cleanup"),
        0
    );

    let published = service
        .upload_attachment(
            problem_id,
            AttachmentKind::Supplement,
            "guide.pdf".into(),
            Some("application/pdf".into()),
            Bytes::from_static(b"published guide"),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("upload attachment for team publication test");
    let team_user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('attachment-team', 'test-hash', 'Attachment Team', 'TEAM', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert team user");
    let team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name) VALUES ('Attachment Team') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(team_user_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("link team account");
    let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Attachment Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert contest");
    sqlx::query(
            "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
        )
        .bind(contest_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("insert contest roster");
    sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
    let team_actor = AuthUser {
        id: team_user_id,
        username: "attachment-team".into(),
        display_name: "Attachment Team".into(),
        user_type: UserType::Team,
        permissions: vec![],
        password_reset_required: false,
    };
    assert!(
        service.download_attachment(problem_id, published.id, &team_actor, &storage).await.is_err()
    );
    sqlx::query("UPDATE contests SET status = 'RUNNING' WHERE id = $1")
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("start contest");
    let team_download = service
        .download_attachment(problem_id, published.id, &team_actor, &storage)
        .await
        .expect("rostered team downloads started contest attachment");
    assert_eq!(team_download.content, Bytes::from_static(b"published guide"));
    assert!(
        service
            .delete_attachment(
                problem_id,
                published.id,
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn contest_manager_must_manage_every_problem_assignment_before_upload(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('scoped-admin', 'test-hash', 'Scoped Admin', 'STAFF', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest admin");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('shared-problem', 'Shared') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    let first_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Managed Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert managed contest");
    let second_contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests (name, status, visibility) VALUES ('Foreign Contest', 'DRAFT', 'PRIVATE') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert foreign contest");
    for (contest_id, alias) in [(first_contest_id, "A"), (second_contest_id, "B")] {
        sqlx::query(
                "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, $3, 1)",
            )
            .bind(contest_id)
            .bind(problem_id)
            .bind(alias)
            .execute(&pool)
            .await
            .expect("assign shared problem");
    }
    sqlx::query("INSERT INTO contest_management_assignments (user_id, contest_id) VALUES ($1, $2)")
        .bind(admin_id)
        .bind(first_contest_id)
        .execute(&pool)
        .await
        .expect("assign first contest scope");
    let actor = AuthUser {
        id: admin_id,
        username: "scoped-admin".into(),
        display_name: "Scoped Admin".into(),
        user_type: UserType::Staff,
        permissions: vec!["CONTEST_MANAGE".into()],
        password_reset_required: false,
    };
    let memory = Arc::new(MemoryStorage::default());
    let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
    let service = ProblemService::new(pool.clone());
    assert!(
        service
            .upload_attachment(
                problem_id,
                AttachmentKind::Sample,
                "sample.txt".into(),
                Some("text/plain".into()),
                Bytes::from_static(b"sample"),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .is_err()
    );
    assert!(memory.objects.lock().expect("memory storage lock").is_empty());

    sqlx::query("INSERT INTO contest_management_assignments (user_id, contest_id) VALUES ($1, $2)")
        .bind(admin_id)
        .bind(second_contest_id)
        .execute(&pool)
        .await
        .expect("assign second contest scope");
    let attachment = service
        .upload_attachment(
            problem_id,
            AttachmentKind::Sample,
            "sample.txt".into(),
            Some("text/plain".into()),
            Bytes::from_static(b"sample"),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("fully scoped admin uploads attachment");
    service
        .upload_testdata(
            problem_id,
            testdata_zip("sample", b"test data"),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("fully scoped admin uploads test data");

    sqlx::query("UPDATE contests SET deleted_at = now() WHERE id IN ($1, $2)")
        .bind(first_contest_id)
        .bind(second_contest_id)
        .execute(&pool)
        .await
        .expect("soft-delete contests");
    assert!(
        service
            .upload_attachment(
                problem_id,
                AttachmentKind::Supplement,
                "guide.pdf".into(),
                Some("application/pdf".into()),
                Bytes::from_static(b"guide"),
                &actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &storage,
            )
            .await
            .is_err()
    );
    assert!(
        service.download_attachment(problem_id, attachment.id, &actor, &storage).await.is_err()
    );
    assert!(service.download_testdata(problem_id, &actor, &storage).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn testdata_versions_are_immutable_and_current_pointer_is_downloadable(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('testdata-admin', 'test-hash', 'Testdata Admin', 'SUPER_ADMIN', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert admin");
    let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems (slug, title, created_by) VALUES ('testdata', 'Test Data', $1) RETURNING id",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("insert problem");
    let actor = AuthUser {
        id: user_id,
        username: "testdata-admin".into(),
        display_name: "Testdata Admin".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let memory = Arc::new(MemoryStorage::default());
    let storage = ObjectStorageHandle::new(memory.clone(), "problems-test".into());
    let service = ProblemService::new(pool.clone());
    let first_content = testdata_zip("1", b"first-version");
    let first = service
        .upload_testdata(
            problem_id,
            first_content.clone(),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("upload first test-data version");
    let second_content = testdata_zip("1", b"second-version");
    let second = service
        .upload_testdata(
            problem_id,
            second_content.clone(),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("upload second test-data version");
    assert_eq!((first.version, second.version), (1, 2));
    assert_eq!((first.case_count, second.case_count), (Some(1), Some(1)));
    assert_ne!(first.sha256, second.sha256);
    let versions = sqlx::query_as::<_, (i32, String)>(
            "SELECT version, object_key FROM problem_testdata_versions WHERE problem_id = $1 ORDER BY version",
        )
        .bind(problem_id)
        .fetch_all(&pool)
        .await
        .expect("load immutable test-data history");
    assert_eq!(versions.len(), 2);
    assert_ne!(versions[0].1, versions[1].1);
    assert_eq!(
        memory.get("problems-test", &versions[0].1).await.expect("first version object remains"),
        first_content
    );
    let download = service
        .download_testdata(problem_id, &actor, &storage)
        .await
        .expect("download current test data");
    assert!(download.filename.ends_with("v2.zip"));
    let download_bytes = axum::body::to_bytes(download.content, 8 * 1024 * 1024)
        .await
        .expect("read current test-data download");
    assert_eq!(download_bytes, second_content);
    let current = sqlx::query_as::<_, (i32, String)>(
        "SELECT testdata_version, testdata_sha256 FROM problems WHERE id = $1",
    )
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("load current test-data pointer");
    assert_eq!(current, (second.version, second.sha256));
    let authoritative = service
        .current_testdata_reference(problem_id)
        .await
        .expect("load authoritative test-data reference");
    assert_eq!(authoritative.version, 2);
    assert_eq!(authoritative.object_key, versions[1].1);
    assert_eq!(authoritative.sha256, current.1);
    assert_eq!(authoritative.case_count, Some(1));
    let history =
        service.list_testdata_versions(problem_id, &actor).await.expect("list test-data history");
    assert_eq!(history.iter().map(|item| item.version).collect::<Vec<_>>(), vec![2, 1]);
    assert!(history[0].active);
    assert!(!history[1].active);
    let first_download = service
        .download_testdata_version(problem_id, 1, &actor, &storage)
        .await
        .expect("download first test-data version");
    assert!(first_download.filename.ends_with("v1.zip"));
    let first_download_bytes = axum::body::to_bytes(first_download.content, 8 * 1024 * 1024)
        .await
        .expect("read first test-data version download");
    assert_eq!(first_download_bytes, first_content);
    let activated = service
        .activate_testdata_version(problem_id, 1, 2, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("activate historical test-data version");
    assert_eq!(activated.version, 1);
    assert!(activated.active);
    assert!(
        service
            .activate_testdata_version(problem_id, 2, 2, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST),)
            .await
            .is_err()
    );
    let third_content = testdata_zip("1", b"third-version");
    let third = service
        .upload_testdata(
            problem_id,
            third_content.clone(),
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("upload after activating historical version");
    assert_eq!(third.version, 3);
    let third_download = service
        .download_testdata(problem_id, &actor, &storage)
        .await
        .expect("download third version");
    let third_download_bytes = axum::body::to_bytes(third_download.content, 8 * 1024 * 1024)
        .await
        .expect("read third test-data download");
    assert_eq!(third_download_bytes, third_content);
    sqlx::query("UPDATE problems SET testdata_sha256 = $2 WHERE id = $1")
        .bind(problem_id)
        .bind("0".repeat(64))
        .execute(&pool)
        .await
        .expect("simulate inconsistent compatibility pointer");
    assert!(service.current_testdata_reference(problem_id).await.is_err());
}
