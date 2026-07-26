use std::{
    collections::{BTreeSet, HashSet},
    io::{Cursor, Read},
};

use bytes::Bytes;
use zip::{CompressionMethod, ZipArchive};

use crate::error::AppError;

const MAX_ENTRIES: usize = 10_000;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_PATH_BYTES: usize = 512;

pub struct ArchiveSummary {
    pub case_count: i32,
}

pub async fn validate(content: Bytes) -> Result<ArchiveSummary, AppError> {
    tokio::task::spawn_blocking(move || validate_sync(&content))
        .await
        .map_err(|error| AppError::internal("join test-data archive validation", error))?
}

fn validate_sync(content: &[u8]) -> Result<ArchiveSummary, AppError> {
    let mut archive = ZipArchive::new(Cursor::new(content))
        .map_err(|_| invalid("must be a structurally valid ZIP archive"))?;
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Err(invalid("must contain between 1 and 10000 entries"));
    }

    let mut paths = HashSet::with_capacity(archive.len());
    let mut declared_total = 0_u64;
    let mut actual_total = 0_u64;
    let mut files = 0_usize;
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry =
            archive.by_index(index).map_err(|_| invalid("contains an unreadable entry"))?;
        let name = entry.name().to_owned();
        if name.is_empty()
            || name.len() > MAX_PATH_BYTES
            || name.contains('\\')
            || name.chars().any(char::is_control)
            || entry.enclosed_name().is_none()
        {
            return Err(invalid("contains an unsafe entry path"));
        }
        if !paths.insert(name.clone()) {
            return Err(invalid("contains duplicate entry paths"));
        }
        if entry.encrypted() {
            return Err(invalid("must not contain encrypted entries"));
        }
        if !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            return Err(invalid("contains an unsupported compression method"));
        }
        if let Some(mode) = entry.unix_mode() {
            let file_type = mode & 0o170000;
            if !matches!(file_type, 0 | 0o040000 | 0o100000) {
                return Err(invalid("must not contain links or special files"));
            }
        }
        if entry.is_dir() {
            return Err(invalid("must not contain directories"));
        }
        let path = entry.enclosed_name().ok_or_else(|| invalid("contains an unsafe entry path"))?;
        if path.components().count() != 1 {
            return Err(invalid("test-case files must be stored at the archive root"));
        }
        let (stem, kind) = name
            .strip_suffix(".in")
            .map(|stem| (stem, "input"))
            .or_else(|| name.strip_suffix(".out").map(|stem| (stem, "output")))
            .ok_or_else(|| invalid("regular files must use paired .in and .out names"))?;
        if stem.is_empty()
            || !stem
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(invalid("test-case names must use safe ASCII characters"));
        }
        if kind == "input" {
            inputs.insert(stem.to_owned());
        } else {
            outputs.insert(stem.to_owned());
        }
        files += 1;
        let declared = entry.size();
        if declared > MAX_ENTRY_BYTES {
            return Err(invalid("contains an entry larger than 256 MiB"));
        }
        declared_total = declared_total
            .checked_add(declared)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| invalid("expands beyond the 1 GiB total limit"))?;
        let compressed = entry.compressed_size();
        if declared > compressed.saturating_mul(MAX_COMPRESSION_RATIO).max(MAX_COMPRESSION_RATIO) {
            return Err(invalid("contains an entry with an excessive compression ratio"));
        }

        let actual =
            std::io::copy(&mut entry.by_ref().take(MAX_ENTRY_BYTES + 1), &mut std::io::sink())
                .map_err(|_| invalid("contains an entry that cannot be decompressed safely"))?;
        if actual > MAX_ENTRY_BYTES || actual != declared {
            return Err(invalid("contains an entry with inconsistent expanded size"));
        }
        actual_total = actual_total
            .checked_add(actual)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| invalid("expands beyond the 1 GiB total limit"))?;
    }
    if files == 0 || actual_total != declared_total {
        return Err(invalid("must contain at least one regular file"));
    }
    if inputs.is_empty() || inputs != outputs {
        return Err(invalid("must contain matching .in and .out files for every test case"));
    }
    Ok(ArchiveSummary {
        case_count: i32::try_from(inputs.len())
            .map_err(|_| invalid("contains too many test cases"))?,
    })
}

fn invalid(message: &'static str) -> AppError {
    AppError::validation("file", message)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use bytes::Bytes;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::validate_sync;

    fn archive(entries: &[(&str, &[u8])]) -> Bytes {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, content) in entries {
            writer.start_file(*name, options).expect("start fixture entry");
            writer.write_all(content).expect("write fixture entry");
        }
        Bytes::from(writer.finish().expect("finish fixture archive").into_inner())
    }

    fn compressed_bomb_fixture() -> Bytes {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for name in ["1.in", "1.out"] {
            writer.start_file(name, options).expect("start compressed fixture entry");
            writer.write_all(&vec![0_u8; 1024 * 1024]).expect("write compressed fixture entry");
        }
        Bytes::from(writer.finish().expect("finish compressed fixture").into_inner())
    }

    #[test]
    fn valid_regular_entries_are_accepted() {
        let content = archive(&[("1.in", b"1\n"), ("1.out", b"2\n")]);
        assert_eq!(validate_sync(&content).expect("valid archive").case_count, 1);
    }

    #[test]
    fn traversal_paths_are_rejected() {
        let traversal = archive(&[("../escape.in", b"bad")]);
        assert!(validate_sync(&traversal).is_err());
    }

    #[test]
    fn missing_pairs_and_nested_cases_are_rejected() {
        assert!(validate_sync(&archive(&[("1.in", b"one")])).is_err());
        assert!(
            validate_sync(&archive(&[("cases/1.in", b"one"), ("cases/1.out", b"one")])).is_err()
        );
    }

    #[test]
    fn excessive_compression_ratio_is_rejected() {
        assert!(validate_sync(&compressed_bomb_fixture()).is_err());
    }
}
