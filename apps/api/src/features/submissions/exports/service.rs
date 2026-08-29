use std::{fs::OpenOptions, net::IpAddr, path::PathBuf};

use bytes::Bytes;
use sha2::{Digest, Sha256};
use zip::ZipWriter;

use crate::{
    error::AppError, features::auth::model::AuthUser, object_storage::ObjectStorageHandle,
};

use crate::features::submissions::exports::helpers::{
    append_zip_entry, build_zip, enforce_sync_source_limit, load_export_rows, metadata_csv,
    record_export_audit, source_manifest_csv, source_manifest_entries_csv, source_path,
    temporary_export_path, write_new_file,
};
use crate::features::submissions::exports::{
    ExportArtifact, ExportRow, MAX_ASYNC_SOURCE_BYTES, MAX_SYNC_METADATA_ROWS,
    MAX_SYNC_SOURCE_BYTES, SourceFile, SourceManifestEntry,
};
use crate::features::submissions::{
    export_tasks::ExportTaskKind, query::require_admin_access, service::SubmissionService,
};

impl SubmissionService {
    pub(crate) async fn generate_export_artifact(
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
            .get_limited(
                storage.source_bucket(),
                &row.source_object_key,
                super::super::model::MAX_SOURCE_BYTES,
            )
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
            .get_limited(
                storage.source_bucket(),
                &row.source_object_key,
                super::super::model::MAX_SOURCE_BYTES,
            )
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
