use std::{
    fs::OpenOptions,
    io::{Cursor, Seek, Write},
    net::IpAddr,
    path::PathBuf,
};

use bytes::Bytes;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::error::AppError;

use super::super::export_tasks::ExportTaskKind;
use super::{
    ExportRow, MAX_SYNC_SOURCE_BYTES, MAX_SYNC_SOURCE_COUNT, SourceFile, SourceManifestEntry,
};

pub(super) fn temporary_export_path(contest_id: i64, kind: ExportTaskKind) -> PathBuf {
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

pub(super) fn write_new_file(path: &std::path::Path, content: &[u8]) -> Result<(), AppError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| AppError::internal("create export temporary file", error))?;
    file.write_all(content)
        .map_err(|error| AppError::internal("write export temporary file", error))
}

pub(super) async fn load_export_rows(
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
        JOIN contests contest ON contest.id = submission.contest_id
                            AND contest.deleted_at IS NULL
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

pub(super) fn enforce_sync_source_limit(rows: &[ExportRow]) -> Result<(), AppError> {
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

pub(super) fn metadata_csv(rows: &[ExportRow]) -> Result<String, AppError> {
    let mut output = "\u{feff}submissionId,contestId,problemId,problemAlias,teamId,teamName,language,sourceSizeBytes,sourceSha256,status,verdict,totalTimeMs,peakMemoryKb,submittedAt,judgedAt,activeJudgementId\r\n".to_string();
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

pub(super) fn source_manifest_csv(
    rows: &[ExportRow],
    files: &[SourceFile],
) -> Result<String, AppError> {
    let mut output =
        "\u{feff}submissionId,teamId,problemAlias,language,submittedAt,path,sha256\r\n".to_string();
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

pub(super) fn source_manifest_entries_csv(
    rows: &[ExportRow],
    entries: &[SourceManifestEntry],
) -> Result<String, AppError> {
    let mut output =
        "\u{feff}submissionId,teamId,problemAlias,language,submittedAt,path,sha256\r\n".to_string();
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

pub(super) fn append_zip_entry<W: Write + Seek + Send + 'static>(
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

pub(super) fn build_zip(files: Vec<SourceFile>, manifest: String) -> Result<Bytes, AppError> {
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

pub(super) fn source_path(row: &ExportRow) -> String {
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

pub(super) fn safe_component(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(32)
        .collect::<String>();
    if value.is_empty() { "problem".into() } else { value }
}

pub(super) fn append_csv_row(output: &mut String, fields: &[String]) {
    output.push_str(&fields.iter().map(|field| csv_field(field)).collect::<Vec<_>>().join(","));
    output.push_str("\r\n");
}

pub(super) fn csv_field(value: &str) -> String {
    let value = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn optional_i32(value: Option<i32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

pub(super) fn format_time(value: OffsetDateTime) -> Result<String, AppError> {
    value.format(&Rfc3339).map_err(|error| AppError::internal("format export timestamp", error))
}

pub(super) fn optional_time(value: Option<OffsetDateTime>) -> Result<String, AppError> {
    value.map(format_time).transpose().map(Option::unwrap_or_default)
}

pub(super) async fn record_export_audit(
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
