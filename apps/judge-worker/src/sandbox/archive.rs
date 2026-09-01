use std::{collections::HashMap, path::Path};

use thiserror::Error;

use crate::sandbox::fs::with_path_context;
use crate::sandbox::{
    MAX_TESTDATA_ARCHIVE_BYTES, MAX_TESTDATA_EXTRACTED_BYTES, MAX_TESTDATA_FILES, SandboxError,
};

/// Failure of the contestant-provided output-only archive. The archive bytes
/// are authenticated as contestant input (SHA-256 verified against the task
/// source), so every validation problem is the submission's fault and must
/// surface as a contestant verdict instead of an infrastructure failure.
#[derive(Debug, Error)]
pub(super) enum OutputArchiveError {
    #[error("output archive is invalid: {0}")]
    Invalid(String),
    #[error("output archive expands beyond the extraction budget")]
    BudgetBreached,
    #[error("artifact cache I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub(super) async fn extract_cases(
    archive: std::path::PathBuf,
    destination: std::path::PathBuf,
) -> Result<usize, SandboxError> {
    tokio::task::spawn_blocking(move || {
        extract_cases_blocking(&archive, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
    })
    .await
    .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?
}

pub(super) async fn extract_output_cases(
    archive: Vec<u8>,
    destination: std::path::PathBuf,
) -> Result<(), OutputArchiveError> {
    tokio::task::spawn_blocking(move || {
        extract_output_cases_blocking(&archive, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
    })
    .await
    // A cancelled extraction task is a worker-side defect, not the submission's.
    .map_err(|error| OutputArchiveError::Io(std::io::Error::other(error.to_string())))?
}

pub(super) fn extract_output_cases_blocking(
    archive: &[u8],
    destination: &Path,
    budget: u64,
) -> Result<(), OutputArchiveError> {
    use std::io::Read;
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|error| OutputArchiveError::Invalid(format!("invalid output archive: {error}")))?;
    if zip.len() > MAX_TESTDATA_FILES {
        return Err(OutputArchiveError::Invalid("output archive has too many files".to_owned()));
    }
    let mut remaining = budget;
    for index in 0..zip.len() {
        let mut entry =
            zip.by_index(index).map_err(|error| OutputArchiveError::Invalid(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .enclosed_name()
            .ok_or_else(|| OutputArchiveError::Invalid("unsafe output archive path".to_owned()))?;
        if name.components().count() != 1
            || name.extension().and_then(std::ffi::OsStr::to_str) != Some("out")
        {
            return Err(OutputArchiveError::Invalid(
                "output archive must contain only root-level .out files".to_owned(),
            ));
        }
        let mut content = Vec::new();
        // Never trust the declared entry size: zip metadata is
        // contestant-controlled and may lie. Inflate at most one byte past the
        // remaining budget so the actual output is bounded no matter what the
        // headers claim.
        let mut limited = (&mut entry).take(remaining.saturating_add(1));
        limited.read_to_end(&mut content).map_err(|error| {
            OutputArchiveError::Invalid(format!("unreadable output entry: {error}"))
        })?;
        let extracted = u64::try_from(content.len()).unwrap_or(u64::MAX);
        if extracted > remaining {
            return Err(OutputArchiveError::BudgetBreached);
        }
        remaining -= extracted;
        let entry_path = destination.join(name.file_name().expect("file name"));
        std::fs::write(&entry_path, content).map_err(|error| {
            OutputArchiveError::Io(crate::sandbox::fs::with_path_context(
                error,
                "write output entry",
                &entry_path,
            ))
        })?;
    }
    Ok(())
}

pub(super) fn extract_cases_blocking(
    archive: &Path,
    destination: &Path,
    budget: u64,
) -> Result<usize, SandboxError> {
    use std::io::Read;

    type CasePair = (Option<Vec<u8>>, Option<Vec<u8>>);

    if std::fs::metadata(archive)
        .map_err(|error| SandboxError::Io(with_path_context(error, "inspect test-data archive", archive)))?
        .len()
        > MAX_TESTDATA_ARCHIVE_BYTES
    {
        return Err(SandboxError::InvalidTestdata("test-data archive is too large".to_owned()));
    }
    let file = std::fs::File::open(archive)
        .map_err(|error| SandboxError::Io(with_path_context(error, "open test-data archive", archive)))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?;
    if zip.len() > MAX_TESTDATA_FILES {
        return Err(SandboxError::InvalidTestdata(
            "test-data archive has too many files".to_owned(),
        ));
    }
    let mut cases: HashMap<String, CasePair> = HashMap::new();
    let mut remaining = budget;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| SandboxError::InvalidTestdata("unsafe archive path".to_owned()))?;
        if enclosed.components().count() != 1 {
            return Err(SandboxError::InvalidTestdata(
                "test cases must be root-level files".to_owned(),
            ));
        }
        let extension = enclosed.extension().and_then(std::ffi::OsStr::to_str);
        if !matches!(extension, Some("in" | "out")) {
            continue;
        }
        let stem = enclosed
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| SandboxError::InvalidTestdata("invalid test-case name".to_owned()))?
            .to_owned();
        let mut content = Vec::new();
        // The declared entry size comes from untrusted zip metadata; bound the
        // actual inflate output so a lying header cannot exhaust worker
        // memory. The classification stays InvalidTestdata: testdata is
        // problem-admin owned, not contestant input.
        let mut limited = (&mut entry).take(remaining.saturating_add(1));
        limited.read_to_end(&mut content).map_err(|error| {
            SandboxError::Io(std::io::Error::other(format!(
                "inflate test-data entry {}: {error}",
                enclosed.display()
            )))
        })?;
        let extracted = u64::try_from(content.len()).unwrap_or(u64::MAX);
        if extracted > remaining {
            return Err(SandboxError::InvalidTestdata(
                "test-data archive expands beyond the limit".to_owned(),
            ));
        }
        remaining -= extracted;
        let pair = cases.entry(stem).or_default();
        match extension {
            Some("in") if pair.0.is_none() => pair.0 = Some(content),
            Some("out") if pair.1.is_none() => pair.1 = Some(content),
            _ => {
                return Err(SandboxError::InvalidTestdata("duplicate test-case side".to_owned()));
            }
        }
    }
    if cases.is_empty() || cases.values().any(|pair| pair.0.is_none() || pair.1.is_none()) {
        return Err(SandboxError::InvalidTestdata(
            "archive must contain paired .in/.out cases".to_owned(),
        ));
    }
    let mut ordered: Vec<_> = cases.into_iter().collect();
    ordered.sort_by(|left, right| left.0.cmp(&right.0));
    for (offset, (_name, (input, output))) in ordered.into_iter().enumerate() {
        let index = offset + 1;
        let input_path = destination.join(format!("{index}.in"));
        std::fs::write(&input_path, input.ok_or_else(|| SandboxError::InvalidTestdata("missing input".to_owned()))?)
            .map_err(|error| SandboxError::Io(with_path_context(error, "write test-case input", &input_path)))?;
        let output_path = destination.join(format!("{index}.out"));
        std::fs::write(&output_path, output.ok_or_else(|| SandboxError::InvalidTestdata("missing output".to_owned()))?)
            .map_err(|error| SandboxError::Io(with_path_context(error, "write test-case output", &output_path)))?;
    }
    Ok(ordered_len(destination)?)
}

fn ordered_len(destination: &Path) -> Result<usize, std::io::Error> {
    let count = std::fs::read_dir(destination)
        .map_err(|error| with_path_context(error, "read extracted test cases", destination))?
        .count();
    Ok(count / 2)
}
