use std::{
    collections::HashMap,
    io::{Cursor, Read},
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use zip::ZipArchive;

use crate::features::submissions::exports::{
    SourceFile,
    helpers::{build_zip, csv_field, safe_component},
};
use crate::{
    features::{
        auth::model::{AuthUser, UserType},
        submissions::SubmissionService,
    },
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
        self.objects
            .lock()
            .expect("memory storage lock")
            .insert((bucket.into(), key.into()), content);
        Ok(())
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.objects
            .lock()
            .expect("memory storage lock")
            .get(&(bucket.into(), key.into()))
            .cloned()
            .ok_or_else(|| ObjectStorageError::Request("not found".into()))
    }

    async fn delete(&self, _bucket: &str, _key: &str) -> Result<(), ObjectStorageError> {
        Ok(())
    }
}

#[test]
fn csv_blocks_formulas_and_escapes_quotes() {
    assert_eq!(csv_field("=cmd|' /C calc'!A0"), "\"'=cmd|' /C calc'!A0\"");
    assert_eq!(csv_field("Team \"A\""), "\"Team \"\"A\"\"\"");
}

#[test]
fn zip_paths_are_fixed_and_archive_contains_manifest() {
    assert_eq!(safe_component("../../A 题"), "A");
    let archive = build_zip(
        vec![SourceFile {
            path: "team-1/problem-A/submission-2.cpp".into(),
            bytes: Bytes::from_static(b"int main() {}"),
            sha256: "hash".into(),
        }],
        "manifest".into(),
    )
    .expect("build archive");
    let mut archive = ZipArchive::new(Cursor::new(archive)).expect("open archive");
    assert_eq!(archive.len(), 2);
    let mut manifest = String::new();
    archive
        .by_name("manifest.csv")
        .expect("manifest entry")
        .read_to_string(&mut manifest)
        .expect("read manifest");
    assert_eq!(manifest, "manifest");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn administrator_exports_verified_sources_and_audits_both_formats(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('export-root', 'test-hash', 'Export Root', 'SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert export administrator");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility) VALUES ('Export Contest', 'DRAFT', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert export contest");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('export-a', 'Export A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert export problem");
    let team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('=SUM(1,1)') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert export team");
    sqlx::query(
        "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
    )
    .bind(contest_id)
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("assign export problem");
    let source = Bytes::from_static(b"int main() { return 0; }");
    let source_hash = hex::encode(Sha256::digest(&source));
    let submission_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO submissions
            (contest_id, problem_id, team_id, language, source_object_key,
             source_size_bytes, source_sha256, status, verdict, judged_at)
        VALUES ($1, $2, $3, 'cpp', 'sources/export.cpp', $4, $5, 'COMPLETED', 'ACCEPTED', now())
        RETURNING id
        "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .bind(i32::try_from(source.len()).expect("source length"))
    .bind(&source_hash)
    .fetch_one(&pool)
    .await
    .expect("insert export submission");
    sqlx::query(
        "INSERT INTO judgements (id, submission_id, verdict, completed_at) VALUES ($1, $2, 'ACCEPTED', now())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(submission_id)
    .execute(&pool)
    .await
    .expect("insert export judgement");
    let memory = Arc::new(MemoryStorage::default());
    memory
        .put("sources", "sources/export.cpp", Some("text/plain"), source.clone())
        .await
        .expect("store export source");
    let storage = ObjectStorageHandle::with_buckets(memory, "problems".into(), "sources".into());
    let actor = AuthUser {
        id: admin_id,
        username: "export-root".into(),
        display_name: "Export Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let service = SubmissionService::new(pool.clone());
    let csv = service
        .export_metadata_csv(contest_id, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("export metadata");
    assert!(csv.contains("ACCEPTED"));
    assert!(csv.contains("\"'=SUM(1,1)\""));
    let zip = service
        .export_sources_zip(contest_id, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST), &storage)
        .await
        .expect("export sources");
    let mut archive = ZipArchive::new(Cursor::new(zip)).expect("open source export");
    let path = format!("team-{team_id}/problem-A/submission-{submission_id}.cpp");
    let mut exported = Vec::new();
    archive
        .by_name(&path)
        .expect("source entry")
        .read_to_end(&mut exported)
        .expect("read source entry");
    assert_eq!(exported, source);
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_logs WHERE actor_user_id = $1 AND action IN ('SUBMISSION_METADATA_EXPORTED', 'SUBMISSION_SOURCES_EXPORTED')",
    )
    .bind(admin_id)
    .fetch_one(&pool)
    .await
    .expect("count export audit entries");
    assert_eq!(audit_count, 2);
}
