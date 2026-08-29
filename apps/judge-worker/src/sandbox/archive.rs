use std::{collections::HashMap, path::Path};

use crate::sandbox::{
    MAX_TESTDATA_ARCHIVE_BYTES, MAX_TESTDATA_EXTRACTED_BYTES, MAX_TESTDATA_FILES, SandboxError,
};

pub(super) async fn extract_cases(
    archive: std::path::PathBuf,
    destination: std::path::PathBuf,
) -> Result<usize, SandboxError> {
    tokio::task::spawn_blocking(move || extract_cases_blocking(&archive, &destination))
        .await
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?
}

pub(super) async fn extract_output_cases(
    archive: Vec<u8>,
    destination: std::path::PathBuf,
) -> Result<(), SandboxError> {
    tokio::task::spawn_blocking(move || extract_output_cases_blocking(&archive, &destination))
        .await
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?
}

pub(super) fn extract_output_cases_blocking(
    archive: &[u8],
    destination: &Path,
) -> Result<(), SandboxError> {
    use std::io::Read;
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader).map_err(|error| {
        SandboxError::InvalidTestdata(format!("invalid output archive: {error}"))
    })?;
    if zip.len() > MAX_TESTDATA_FILES {
        return Err(SandboxError::InvalidTestdata("output archive has too many files".to_owned()));
    }
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.enclosed_name().ok_or_else(|| {
            SandboxError::InvalidTestdata("unsafe output archive path".to_owned())
        })?;
        if name.components().count() != 1
            || name.extension().and_then(std::ffi::OsStr::to_str) != Some("out")
        {
            return Err(SandboxError::InvalidTestdata(
                "output archive must contain only root-level .out files".to_owned(),
            ));
        }
        if entry.size() > MAX_TESTDATA_EXTRACTED_BYTES {
            return Err(SandboxError::InvalidTestdata("output file is too large".to_owned()));
        }
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?;
        std::fs::write(destination.join(name.file_name().expect("file name")), content)
            .map_err(SandboxError::Io)?;
    }
    Ok(())
}

fn extract_cases_blocking(archive: &Path, destination: &Path) -> Result<usize, SandboxError> {
    use std::io::Read;

    type CasePair = (Option<Vec<u8>>, Option<Vec<u8>>);

    if std::fs::metadata(archive)?.len() > MAX_TESTDATA_ARCHIVE_BYTES {
        return Err(SandboxError::InvalidTestdata("test-data archive is too large".to_owned()));
    }
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?;
    if zip.len() > MAX_TESTDATA_FILES {
        return Err(SandboxError::InvalidTestdata(
            "test-data archive has too many files".to_owned(),
        ));
    }
    let mut cases: HashMap<String, CasePair> = HashMap::new();
    let mut extracted_bytes = 0_u64;
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
        extracted_bytes = extracted_bytes.saturating_add(entry.size());
        if extracted_bytes > MAX_TESTDATA_EXTRACTED_BYTES {
            return Err(SandboxError::InvalidTestdata(
                "test-data archive expands beyond the limit".to_owned(),
            ));
        }
        let stem = enclosed
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| SandboxError::InvalidTestdata("invalid test-case name".to_owned()))?
            .to_owned();
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
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
        std::fs::write(
            destination.join(format!("{index}.in")),
            input.ok_or_else(|| SandboxError::InvalidTestdata("missing input".to_owned()))?,
        )?;
        std::fs::write(
            destination.join(format!("{index}.out")),
            output.ok_or_else(|| SandboxError::InvalidTestdata("missing output".to_owned()))?,
        )?;
    }
    Ok(ordered_len(destination)?)
}

fn ordered_len(destination: &Path) -> Result<usize, std::io::Error> {
    Ok(std::fs::read_dir(destination)?.count() / 2)
}
