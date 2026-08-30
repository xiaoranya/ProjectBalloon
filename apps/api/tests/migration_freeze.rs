//! Freezes the SQL migration set: editing an existing migration or adding a
//! new one must be reflected in `migrations/FROZEN.txt` on purpose. A silent
//! history edit therefore fails CI instead of diverging deployed databases.
//!
//! Intended workflow: when a migration change is deliberate, recompute the
//! manifest (`sha256sum migrations/*.sql > migrations/FROZEN.txt`, keeping the
//! `<checksum> <name>` lines) and include it in the same review.

use sha2::{Digest, Sha256};
use std::path::Path;

#[test]
fn migrations_match_the_frozen_manifest() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/FROZEN.txt");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read migrations/FROZEN.txt");
    let migrations_dir = manifest_path.parent().expect("migrations directory");

    let mut expected: Vec<(String, String)> = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (checksum, name) =
            line.split_once(' ').expect("manifest lines use `<sha256> <filename>`");
        expected.push((name.to_owned(), checksum.to_ascii_lowercase()));
    }
    assert!(!expected.is_empty(), "the manifest must list at least one migration");

    let mut actual: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(migrations_dir).expect("read migrations directory") {
        let path = entry.expect("migration entry").path();
        let name = path.file_name().expect("file name").to_string_lossy().into_owned();
        if name == "FROZEN.txt" || name == "README.md" {
            continue;
        }
        assert!(
            path.is_file() && name.ends_with(".sql"),
            "unexpected non-SQL file in migrations: {name}"
        );
        let content = std::fs::read(&path).expect("read migration file");
        actual.push((name, hex::encode(Sha256::digest(content))));
    }
    actual.sort();

    let mut frozen: Vec<(String, String)> = expected;
    frozen.sort();
    assert_eq!(
        actual, frozen,
        "migrations drifted from FROZEN.txt; recompute the manifest only if the change is intended"
    );
}
