use std::io::Write;

use super::archive::extract_output_cases_blocking;
use super::compare::standard_output_matches;
use super::metrics::{GnuTimeMetrics, extract_gnu_time_metrics};

#[test]
fn standard_comparison_normalizes_line_endings_and_trailing_line_space() {
    assert!(standard_output_matches(b"one  \r\ntwo\r\n", b"one\ntwo\n\n"));
    assert!(!standard_output_matches(b"one two\n", b"one  two\n"));
}

#[test]
fn extracts_trailing_gnu_time_metrics_and_preserves_program_stderr() {
    let (logs, metrics) = extract_gnu_time_metrics(
        "contestant diagnostic\n__PROJECT_BALLOON_GNU_TIME__ 0.12 0.03 4096\n",
    );
    assert_eq!(logs, "contestant diagnostic");
    assert_eq!(metrics, Some(GnuTimeMetrics { cpu_time_ms: 150, peak_memory_kb: 4096 }));
}

#[test]
fn ignores_forged_non_trailing_gnu_time_metrics() {
    let original = "__PROJECT_BALLOON_GNU_TIME__ 0.00 0.00 1\ncontestant diagnostic\n";
    let (logs, metrics) = extract_gnu_time_metrics(original);
    assert_eq!(logs, original);
    assert_eq!(metrics, None);
}

#[test]
fn extracts_gnu_time_when_program_stderr_has_no_trailing_newline() {
    let (logs, metrics) = extract_gnu_time_metrics(
        "contestant diagnostic__PROJECT_BALLOON_GNU_TIME__ 0.12 0.03 4096\n",
    );
    assert_eq!(logs, "contestant diagnostic");
    assert_eq!(metrics, Some(GnuTimeMetrics { cpu_time_ms: 150, peak_memory_kb: 4096 }));
}

#[test]
fn output_only_archive_accepts_root_level_outputs() {
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file("1.out", zip::write::SimpleFileOptions::default()).expect("entry");
        zip.write_all(b"42\n").expect("output");
        zip.finish().expect("zip");
    }
    let destination = std::env::temp_dir().join(format!("pb-output-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&destination).expect("destination");
    extract_output_cases_blocking(&bytes, &destination).expect("extract output");
    assert_eq!(std::fs::read(destination.join("1.out")).expect("read"), b"42\n");
    std::fs::remove_dir_all(destination).expect("cleanup");
}
