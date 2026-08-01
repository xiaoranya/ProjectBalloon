use std::{
    fs::OpenOptions,
    io::{Cursor, Seek, Write},
    net::IpAddr,
    path::PathBuf,
};

use bytes::Bytes;
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::{
    error::AppError, features::auth::model::AuthUser, object_storage::ObjectStorageHandle,
};

use super::{
    export_tasks::ExportTaskKind, query::require_admin_access, service::SubmissionService,
};

const MAX_SYNC_SOURCE_COUNT: i64 = 10_000;
const MAX_SYNC_SOURCE_BYTES: i64 = 128 * 1024 * 1024;
// The synchronous metadata CSV is built entirely in memory, so cap the row
// count the same way the synchronous source export is capped. Larger exports
// should use the async export-task path, which streams to a temp file.
const MAX_SYNC_METADATA_ROWS: i64 = 10_000;
const MAX_ASYNC_SOURCE_BYTES: i64 = 2 * 1024 * 1024 * 1024;

#[derive(sqlx::FromRow)]
struct ExportRow {
    id: i64,
    contest_id: i64,
    problem_id: i64,
    problem_alias: String,
    team_id: i64,
    team_name: String,
    language: String,
    source_size_bytes: i32,
    source_sha256: Option<String>,
    status: String,
    verdict: Option<String>,
    total_time_ms: Option<i32>,
    peak_memory_kb: Option<i32>,
    submitted_at: OffsetDateTime,
    judged_at: Option<OffsetDateTime>,
    active_judgement_id: Option<Uuid>,
    source_object_key: String,
}

struct SourceFile {
    path: String,
    bytes: Bytes,
    sha256: String,
}

struct SourceManifestEntry {
    path: String,
    sha256: String,
}

pub(super) struct ExportArtifact {
    pub path: PathBuf,
    pub extension: &'static str,
    pub content_type: &'static str,
}

impl SubmissionService {
    pub(super) async fn generate_export_artifact(
        &self,
        contest_id: i64,
        kind: ExportTaskKind,
        storage: &ObjectStorageHandle,
    ) -> Result<ExportArtifact, AppError> {
        let rows = load_export_rows(&self.database, contest_id).await?;
        let path = temporary_export_path(contest_id, kind);
        let result = async {
            match kind {
                ExportTaskKind::MetadataCsv => {
                    let output_path = path.clone();
                    tokio::task::spawn_blocking(move || {
                        let csv = metadata_csv(&rows)?;
                        write_new_file(&output_path, csv.as_bytes())
                    })
                    .await
                    .map_err(|error| AppError::internal("join metadata export task", error))??;
                    Ok(ExportArtifact {
                        path: path.clone(),
                        extension: "csv",
                        content_type: "text/csv; charset=utf-8",
                    })
                }
                ExportTaskKind::SourcesZip => {
                    build_sources_archive_file(rows, storage, path.clone()).await?;
                    Ok(ExportArtifact {
                        path: path.clone(),
                        extension: "zip",
                        content_type: "application/zip",
                    })
                }
            }
        }
        .await;
        if result.is_err() {
            let _ignored = tokio::fs::remove_file(path).await;
        }
        result
    }

    pub async fn export_metadata_csv(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<String, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let rows = load_export_rows(&self.database, contest_id).await?;
        let count = i64::try_from(rows.len())
            .map_err(|error| AppError::internal("convert metadata export count", error))?;
        if count > MAX_SYNC_METADATA_ROWS {
            return Err(AppError::conflict(
                "SOURCE_EXPORT_TOO_LARGE",
                "Synchronous metadata export is limited to 10,000 submissions; use the async export task for larger contests",
            ));
        }
        let csv = tokio::task::spawn_blocking(move || metadata_csv(&rows))
            .await
            .map_err(|error| AppError::internal("join submission metadata export task", error))??;
        record_export_audit(
            &self.database,
            contest_id,
            actor.id,
            request_ip,
            "SUBMISSION_METADATA_EXPORTED",
        )
        .await?;
        Ok(csv)
    }

    pub async fn export_sources_zip(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
        storage: &ObjectStorageHandle,
    ) -> Result<Bytes, AppError> {
        require_admin_access(&self.database, contest_id, actor).await?;
        let rows = load_export_rows(&self.database, contest_id).await?;
        enforce_sync_source_limit(&rows)?;
        let archive = build_sources_archive(rows, storage).await?;
        record_export_audit(
            &self.database,
            contest_id,
            actor.id,
            request_ip,
            "SUBMISSION_SOURCES_EXPORTED",
        )
        .await?;
        Ok(archive)
    }
}

async fn build_sources_archive(
    rows: Vec<ExportRow>,
    storage: &ObjectStorageHandle,
) -> Result<Bytes, AppError> {
    let files = load_source_files(&rows, storage, Some(MAX_SYNC_SOURCE_BYTES)).await?;
    let archive = tokio::task::spawn_blocking(move || {
        let manifest = source_manifest_csv(&rows, &files)?;
        build_zip(files, manifest)
    })
    .await
    .map_err(|error| AppError::internal("join source export archive task", error))??;
    Ok(archive)
}

async fn build_sources_archive_file(
    rows: Vec<ExportRow>,
    storage: &ObjectStorageHandle,
    path: PathBuf,
) -> Result<(), AppError> {
    let expected_bytes = rows
        .iter()
        .try_fold(0_i64, |total, row| total.checked_add(i64::from(row.source_size_bytes)));
    if expected_bytes.is_none_or(|bytes| bytes > MAX_ASYNC_SOURCE_BYTES) {
        return Err(AppError::conflict(
            "SOURCE_EXPORT_TOO_LARGE",
            "Async source export is limited to 2 GiB",
        ));
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| AppError::internal("create source export temporary file", error))?;
    let mut writer = ZipWriter::new(file);
    let mut entries = Vec::with_capacity(rows.len());
    for row in &rows {
        let bytes = storage
            .backend()
            .get(storage.source_bucket(), &row.source_object_key)
            .await
            .map_err(|error| AppError::internal("download source for export", error))?;
        let actual_size = i64::try_from(bytes.len())
            .map_err(|error| AppError::internal("convert exported source size", error))?;
        if actual_size != i64::from(row.source_size_bytes) {
            return Err(AppError::conflict(
                "SUBMISSION_SOURCE_SIZE_MISMATCH",
                "A stored submission source does not match its recorded size",
            ));
        }
        let sha256 = hex::encode(Sha256::digest(&bytes));
        if row.source_sha256.as_ref().is_some_and(|expected| expected != &sha256) {
            return Err(AppError::conflict(
                "SUBMISSION_SOURCE_HASH_MISMATCH",
                "A stored submission source failed integrity verification",
            ));
        }
        let path_name = source_path(row);
        let entry = SourceManifestEntry { path: path_name.clone(), sha256 };
        writer = tokio::task::spawn_blocking(move || append_zip_entry(writer, &path_name, &bytes))
            .await
            .map_err(|error| AppError::internal("join source export entry task", error))??;
        entries.push(entry);
    }
    let manifest = source_manifest_entries_csv(&rows, &entries)?;
    tokio::task::spawn_blocking(move || {
        writer = append_zip_entry(writer, "manifest.csv", manifest.as_bytes())?;
        writer
            .finish()
            .map(|_| ())
            .map_err(|error| AppError::internal("finish source export file", error))
    })
    .await
    .map_err(|error| AppError::internal("join source export finish task", error))?
}

async fn load_source_files(
    rows: &[ExportRow],
    storage: &ObjectStorageHandle,
    max_bytes: Option<i64>,
) -> Result<Vec<SourceFile>, AppError> {
    let mut files = Vec::with_capacity(rows.len());
    let mut downloaded_bytes = 0_i64;
    for row in rows {
        let bytes = storage
            .backend()
            .get(storage.source_bucket(), &row.source_object_key)
            .await
            .map_err(|error| AppError::internal("download source for export", error))?;
        let actual_size = i64::try_from(bytes.len())
            .map_err(|error| AppError::internal("convert exported source size", error))?;
        downloaded_bytes = downloaded_bytes.checked_add(actual_size).ok_or_else(|| {
            AppError::conflict("SOURCE_EXPORT_TOO_LARGE", "Source export size overflowed")
        })?;
        if actual_size != i64::from(row.source_size_bytes) {
            return Err(AppError::conflict(
                "SUBMISSION_SOURCE_SIZE_MISMATCH",
                "A stored submission source does not match its recorded size",
            ));
        }
        if max_bytes.is_some_and(|limit| downloaded_bytes > limit) {
            return Err(AppError::conflict(
                "SOURCE_EXPORT_TOO_LARGE",
                "Synchronous source export is limited to 128 MiB",
            ));
        }
        let sha256 = hex::encode(Sha256::digest(&bytes));
        if row.source_sha256.as_ref().is_some_and(|expected| expected != &sha256) {
            return Err(AppError::conflict(
                "SUBMISSION_SOURCE_HASH_MISMATCH",
                "A stored submission source failed integrity verification",
            ));
        }
        files.push(SourceFile { path: source_path(row), bytes, sha256 });
    }
    Ok(files)
}

fn temporary_export_path(contest_id: i64, kind: ExportTaskKind) -> PathBuf {
    let extension = match kind {
        ExportTaskKind::MetadataCsv => "csv",
        ExportTaskKind::SourcesZip => "zip",
    };
    std::env::temp_dir().join(format!(
        "project-balloon-export-{contest_id}-{}.{}",
        Uuid::new_v4(),
        extension
    ))
}

fn write_new_file(path: &std::path::Path, content: &[u8]) -> Result<(), AppError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| AppError::internal("create export temporary file", error))?;
    file.write_all(content)
        .map_err(|error| AppError::internal("write export temporary file", error))
}

async fn load_export_rows(
    database: &sqlx::PgPool,
    contest_id: i64,
) -> Result<Vec<ExportRow>, AppError> {
    sqlx::query_as::<_, ExportRow>(
        r#"
        SELECT submission.id, submission.contest_id, submission.problem_id,
               assignment.alias AS problem_alias, submission.team_id,
               team.name AS team_name, submission.language, submission.source_size_bytes,
               submission.source_sha256, submission.status, judgement.verdict,
               judgement.total_time_ms, judgement.peak_memory_kb, submission.submitted_at,
               submission.judged_at, judgement.id AS active_judgement_id,
               submission.source_object_key
        FROM submissions submission
        JOIN teams team ON team.id = submission.team_id
        JOIN contest_problems assignment
          ON assignment.contest_id = submission.contest_id
         AND assignment.problem_id = submission.problem_id
        LEFT JOIN judgements judgement
          ON judgement.submission_id = submission.id
         AND judgement.active_marker IS TRUE
        WHERE submission.contest_id = $1
        ORDER BY submission.submitted_at, submission.id
        "#,
    )
    .bind(contest_id)
    .fetch_all(database)
    .await
    .map_err(|error| AppError::internal("load submissions for export", error))
}

fn enforce_sync_source_limit(rows: &[ExportRow]) -> Result<(), AppError> {
    let count = i64::try_from(rows.len())
        .map_err(|error| AppError::internal("convert source export count", error))?;
    let bytes = rows.iter().map(|row| i64::from(row.source_size_bytes)).sum::<i64>();
    if count > MAX_SYNC_SOURCE_COUNT || bytes > MAX_SYNC_SOURCE_BYTES {
        return Err(AppError::conflict(
            "SOURCE_EXPORT_TOO_LARGE",
            "Synchronous source export is limited to 10,000 files and 128 MiB",
        ));
    }
    Ok(())
}

fn metadata_csv(rows: &[ExportRow]) -> Result<String, AppError> {
    let mut output = String::from(
        "\u{feff}submissionId,contestId,problemId,problemAlias,teamId,teamName,language,sourceSizeBytes,sourceSha256,status,verdict,totalTimeMs,peakMemoryKb,submittedAt,judgedAt,activeJudgementId\r\n",
    );
    for row in rows {
        let fields = [
            row.id.to_string(),
            row.contest_id.to_string(),
            row.problem_id.to_string(),
            row.problem_alias.clone(),
            row.team_id.to_string(),
            row.team_name.clone(),
            row.language.clone(),
            row.source_size_bytes.to_string(),
            row.source_sha256.clone().unwrap_or_default(),
            row.status.clone(),
            row.verdict.clone().unwrap_or_default(),
            optional_i32(row.total_time_ms),
            optional_i32(row.peak_memory_kb),
            format_time(row.submitted_at)?,
            optional_time(row.judged_at)?,
            row.active_judgement_id.map(|value| value.to_string()).unwrap_or_default(),
        ];
        append_csv_row(&mut output, &fields);
    }
    Ok(output)
}

fn source_manifest_csv(rows: &[ExportRow], files: &[SourceFile]) -> Result<String, AppError> {
    let mut output = String::from(
        "\u{feff}submissionId,teamId,problemAlias,language,submittedAt,path,sha256\r\n",
    );
    for (row, file) in rows.iter().zip(files) {
        append_csv_row(
            &mut output,
            &[
                row.id.to_string(),
                row.team_id.to_string(),
                row.problem_alias.clone(),
                row.language.clone(),
                format_time(row.submitted_at)?,
                file.path.clone(),
                file.sha256.clone(),
            ],
        );
    }
    Ok(output)
}

fn source_manifest_entries_csv(
    rows: &[ExportRow],
    entries: &[SourceManifestEntry],
) -> Result<String, AppError> {
    let mut output = String::from(
        "\u{feff}submissionId,teamId,problemAlias,language,submittedAt,path,sha256\r\n",
    );
    for (row, entry) in rows.iter().zip(entries) {
        append_csv_row(
            &mut output,
            &[
                row.id.to_string(),
                row.team_id.to_string(),
                row.problem_alias.clone(),
                row.language.clone(),
                format_time(row.submitted_at)?,
                entry.path.clone(),
                entry.sha256.clone(),
            ],
        );
    }
    Ok(output)
}

fn append_zip_entry<W: Write + Seek + Send + 'static>(
    mut writer: ZipWriter<W>,
    path: &str,
    bytes: &[u8],
) -> Result<ZipWriter<W>, AppError> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    writer
        .start_file(path, options)
        .and_then(|()| writer.write_all(bytes).map_err(zip::result::ZipError::Io))
        .map_err(|error| AppError::internal("write source export entry", error))?;
    Ok(writer)
}

fn build_zip(files: Vec<SourceFile>, manifest: String) -> Result<Bytes, AppError> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    writer
        .start_file("manifest.csv", options)
        .and_then(|()| writer.write_all(manifest.as_bytes()).map_err(zip::result::ZipError::Io))
        .map_err(|error| AppError::internal("write source export manifest", error))?;
    for file in files {
        writer
            .start_file(file.path, options)
            .and_then(|()| writer.write_all(&file.bytes).map_err(zip::result::ZipError::Io))
            .map_err(|error| AppError::internal("write source export entry", error))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| AppError::internal("finish source export archive", error))?;
    Ok(Bytes::from(cursor.into_inner()))
}

fn source_path(row: &ExportRow) -> String {
    let extension = match row.language.as_str() {
        "c" => "c",
        "cpp" => "cpp",
        "java" => "java",
        "python" => "py",
        _ => "txt",
    };
    format!(
        "team-{}/problem-{}/submission-{}.{}",
        row.team_id,
        safe_component(&row.problem_alias),
        row.id,
        extension
    )
}

fn safe_component(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(32)
        .collect::<String>();
    if value.is_empty() { "problem".into() } else { value }
}

fn append_csv_row(output: &mut String, fields: &[String]) {
    output.push_str(&fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","));
    output.push_str("\r\n");
}

fn csv_field(value: &str) -> String {
    let value = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn optional_i32(value: Option<i32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn format_time(value: OffsetDateTime) -> Result<String, AppError> {
    value.format(&Rfc3339).map_err(|error| AppError::internal("format export timestamp", error))
}

fn optional_time(value: Option<OffsetDateTime>) -> Result<String, AppError> {
    value.map(format_time).transpose().map(Option::unwrap_or_default)
}

async fn record_export_audit(
    database: &sqlx::PgPool,
    contest_id: i64,
    actor_id: i64,
    request_ip: IpAddr,
    action: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'CONTEST', $3, $4, 'success')",
    )
    .bind(actor_id)
    .bind(action)
    .bind(contest_id.to_string())
    .bind(request_ip.to_string())
    .execute(database)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record submission export audit", error))
}

#[cfg(test)]
mod tests {
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

    use super::{SourceFile, build_zip, csv_field, safe_component};
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
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name) VALUES ('=SUM(1,1)') RETURNING id",
        )
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
                 source_size_bytes, source_sha256, status, judged_at)
            VALUES ($1, $2, $3, 'cpp', 'sources/export.cpp', $4, $5, 'ACCEPTED', now())
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
        let storage =
            ObjectStorageHandle::with_buckets(memory, "problems".into(), "sources".into());
        let actor = AuthUser {
            id: admin_id,
            username: "export-root".into(),
            display_name: "Export Root".into(),
            user_type: UserType::SuperAdmin,
            roles: Vec::new(),
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
}
