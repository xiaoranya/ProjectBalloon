use std::io::Write;

use project_balloon_contracts::{
    JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeTask, JudgeVerdict,
};
use uuid::Uuid;

use crate::sandbox::MAX_TESTDATA_EXTRACTED_BYTES;
use crate::sandbox::archive::{
    OutputArchiveError, extract_cases_blocking, extract_output_cases_blocking,
};
use crate::sandbox::compare::standard_output_matches;
use crate::sandbox::fs::{read_regular_output_no_follow, truncate_log};
use crate::sandbox::language::LanguageConfig;
use crate::sandbox::metrics::{GnuTimeMetrics, extract_gnu_time_metrics, nonzero_milliseconds};
use crate::sandbox::runner::run_output_only;

fn task_with_language(language: &str) -> JudgeTask {
    let mut task = project_balloon_test_support::valid_judge_task();
    task.language = language.to_owned();
    task
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        for (name, content) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).expect("entry");
            zip.write_all(content).expect("content");
        }
        zip.finish().expect("zip");
    }
    bytes
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pb-{label}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

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
fn a_marker_after_the_report_wins_so_the_report_must_stay_terminal() {
    // The parser trusts the LAST marker. Both judge shells therefore have to
    // keep the real report as the terminal write on the exec stderr stream;
    // contestant-writable bytes must never be concatenated after it.
    let (logs, metrics) = extract_gnu_time_metrics(
        "__PROJECT_BALLOON_GNU_TIME__ 0.30 0.05 8192\n__PROJECT_BALLOON_GNU_TIME__ 0.00 0.00 0\n",
    );
    assert_eq!(logs, "__PROJECT_BALLOON_GNU_TIME__ 0.30 0.05 8192");
    assert_eq!(metrics, Some(GnuTimeMetrics { cpu_time_ms: 0, peak_memory_kb: 0 }));
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
    extract_output_cases_blocking(&bytes, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
        .expect("extract output");
    assert_eq!(std::fs::read(destination.join("1.out")).expect("read"), b"42\n");
    std::fs::remove_dir_all(destination).expect("cleanup");
}

#[test]
fn language_config_maps_every_supported_language() {
    for (language, source, compile_prefix) in [
        ("c", "main.c", "gcc"),
        ("cpp", "main.cpp", "g++"),
        ("java", "Main.java", "javac"),
        ("python", "main.py", "python3"),
    ] {
        let config = LanguageConfig::for_task(&task_with_language(language)).expect("supported");
        assert_eq!(config.source_filename(), source);
        assert_eq!(config.compile_command()[0], compile_prefix);
    }
    assert!(LanguageConfig::for_task(&task_with_language("rust")).is_err());
    assert!(LanguageConfig::for_task(&task_with_language("")).is_err());
}

#[test]
fn run_commands_harden_the_runtimes() {
    let compiled = LanguageConfig::for_task(&task_with_language("cpp")).expect("cpp");
    assert_eq!(compiled.run_command(256), "/work/program");
    let java = LanguageConfig::for_task(&task_with_language("java")).expect("java");
    assert_eq!(java.run_command(256), "java -Xms16m -Xmx128m -cp /work Main");
    assert!(java.run_command(8).contains("-Xmx16m"), "the Java heap must never drop below 16 MiB");
    let python = LanguageConfig::for_task(&task_with_language("python")).expect("python");
    assert!(
        python.run_command(256).starts_with("python3 -I -B "),
        "Python must run in isolated, non-bytecode-caching mode"
    );
}

#[test]
fn truncate_log_cuts_on_char_boundaries() {
    assert_eq!(truncate_log("hello", 5), "hello");
    assert_eq!(truncate_log("hello", 3), "hel");
    assert_eq!(truncate_log("hello", 0), "");
    // Multi-byte characters must never be split mid-codepoint.
    assert_eq!(truncate_log("中文中文", 5), "中");
    assert_eq!(truncate_log("中文", 6), "中文");
}

#[test]
fn output_reads_reject_symlinks_directories_and_missing_files() {
    let dir = temp_dir("fs");
    let regular = dir.join("actual.out");
    std::fs::write(&regular, b"42\n").expect("write output");
    assert_eq!(read_regular_output_no_follow(&regular).expect("read"), Some(b"42\n".to_vec()));
    assert_eq!(read_regular_output_no_follow(&dir.join("missing")).expect("missing"), None);
    let nested = dir.join("nested");
    std::fs::create_dir_all(&nested).expect("nested dir");
    assert_eq!(read_regular_output_no_follow(&nested).expect("directory"), None);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&regular, dir.join("link")).expect("symlink");
        assert_eq!(
            read_regular_output_no_follow(&dir.join("link")).expect("symlink"),
            None,
            "O_NOFOLLOW must treat symlinks as no output"
        );
    }
    std::fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn comparison_ignores_trailing_blank_lines_and_bare_carriage_returns() {
    assert!(standard_output_matches(b"a\r\rb\n", b"a\n\nb\n"));
    assert!(standard_output_matches(b"", b""));
    assert!(standard_output_matches(b"  \n\t\n", b"\n\n"));
    // Binary bytes are compared verbatim; a trailing NUL byte is significant.
    assert!(standard_output_matches(b"value\0", b"value\0"));
    assert!(!standard_output_matches(b"value\0", b"value"));
}

#[test]
fn nonzero_milliseconds_rounds_up_and_saturates() {
    assert_eq!(nonzero_milliseconds(0), None);
    assert_eq!(nonzero_milliseconds(1), Some(1));
    assert_eq!(nonzero_milliseconds(999_999), Some(1));
    assert_eq!(nonzero_milliseconds(1_500_000), Some(2));
    assert_eq!(nonzero_milliseconds(u64::MAX), Some(i32::MAX));
}

#[test]
fn gnu_time_metrics_reject_negative_and_malformed_fields() {
    for logs in [
        "__PROJECT_BALLOON_GNU_TIME__ -0.10 -0.03 4096\n",
        "__PROJECT_BALLOON_GNU_TIME__ 0.10 broken 4096\n",
        "__PROJECT_BALLOON_GNU_TIME__ 0.10 0.03\n",
        "__PROJECT_BALLOON_GNU_TIME__ 0.10 0.03 4096 extra\n",
    ] {
        let (logs_back, metrics) = extract_gnu_time_metrics(logs);
        assert_eq!(metrics, None, "{logs}");
        assert_eq!(logs_back, logs);
    }
}

#[test]
fn testdata_extraction_pairs_cases_and_renumbers_them_in_order() {
    let work = temp_dir("cases");
    let archive_path = work.join("testdata.zip");
    // Stems sort lexicographically: "10" before "2". A non .in/.out file is ignored.
    std::fs::write(
        &archive_path,
        zip_bytes(&[
            ("2.in", b"second"),
            ("10.out", b"ten"),
            ("10.in", b"ten"),
            ("2.out", b"second-out"),
            ("notes.txt", b"ignored"),
        ]),
    )
    .expect("archive file");
    let destination = work.join("cases");
    std::fs::create_dir_all(&destination).expect("destination");

    let count = extract_cases_blocking(&archive_path, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
        .expect("extract cases");
    assert_eq!(count, 2);
    assert_eq!(std::fs::read(destination.join("1.in")).expect("case 1 input"), b"ten");
    assert_eq!(std::fs::read(destination.join("1.out")).expect("case 1 output"), b"ten");
    assert_eq!(std::fs::read(destination.join("2.in")).expect("case 2 input"), b"second");
    assert_eq!(std::fs::read(destination.join("2.out")).expect("case 2 output"), b"second-out");
    assert!(!destination.join("3.in").exists());
    std::fs::remove_dir_all(work).expect("cleanup");
}

#[test]
fn testdata_extraction_rejects_unpaired_and_nested_cases() {
    let work = temp_dir("badcases");
    let fixtures: Vec<Vec<(&str, &[u8])>> =
        vec![vec![("1.in", b"x")], vec![("sub/1.in", b"x"), ("sub/1.out", b"y")]];
    for entries in fixtures {
        let archive_path = work.join(format!("{}.zip", Uuid::new_v4()));
        std::fs::write(&archive_path, zip_bytes(&entries)).expect("archive file");
        let destination = work.join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&destination).expect("destination");
        assert!(
            extract_cases_blocking(&archive_path, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
                .is_err()
        );
    }
    std::fs::remove_dir_all(work).expect("cleanup");
}

/// Builds a Stored-method zip (deterministic header layout) from entries.
fn stored_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            zip.start_file(*name, options).expect("entry");
            zip.write_all(content).expect("content");
        }
        zip.finish().expect("zip");
    }
    bytes
}

/// Patches the declared uncompressed size in every local and central header to
/// `declared`, simulating a zip whose metadata lies about its inflate size.
fn patch_declared_sizes(archive: &mut [u8], declared: u32) {
    const LOCAL_HEADER: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
    const CENTRAL_HEADER: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    let mut patched = 0;
    for window in 0..archive.len().saturating_sub(4) {
        let header = &archive[window..window + 4];
        let at = if header == LOCAL_HEADER.as_slice() {
            window + 22
        } else if header == CENTRAL_HEADER.as_slice() {
            window + 24
        } else {
            continue;
        };
        archive[at..at + 4].copy_from_slice(&declared.to_le_bytes());
        patched += 1;
    }
    assert!(patched >= 2, "expected to patch local and central headers");
}

fn output_only_task() -> JudgeTask {
    let mut task = project_balloon_test_support::valid_judge_task();
    task.judge_mode = project_balloon_contracts::JudgeMode::OutputOnly;
    task.language = "output".to_owned();
    task
}

#[test]
fn output_extraction_never_trusts_declared_entry_sizes() {
    // The headers claim 4 bytes; the actual inflate produces 2 KiB. Extraction
    // must read what is really there, not what the metadata promises.
    let mut archive = stored_zip_bytes(&[("1.out", &[b'A'; 2_048])]);
    patch_declared_sizes(&mut archive, 4);
    let destination = temp_dir("declared-lie");
    extract_output_cases_blocking(&archive, &destination, MAX_TESTDATA_EXTRACTED_BYTES)
        .expect("declared size must not bound the actual read");
    assert_eq!(std::fs::read(destination.join("1.out")).expect("read"), vec![b'A'; 2_048],);
    std::fs::remove_dir_all(destination).expect("cleanup");
}

#[test]
fn output_extraction_budget_caps_actual_bytes_and_is_cumulative() {
    // Headers claim 4 bytes per entry, the real content is 4 KiB, and the
    // budget is 1 KiB: the inflate must stop at the budget, not at the lie.
    let mut lying = stored_zip_bytes(&[("1.out", &[b'B'; 4_096])]);
    patch_declared_sizes(&mut lying, 4);
    let destination = temp_dir("budget-breach");
    let error =
        extract_output_cases_blocking(&lying, &destination, 1_024).expect_err("budget breach");
    assert!(matches!(error, OutputArchiveError::BudgetBreached), "unexpected error: {error:?}");
    assert!(!destination.join("1.out").exists(), "no partial file may be written");

    // Two entries that each fit a 1 KiB budget must breach it together.
    let mut cumulative = stored_zip_bytes(&[("1.out", &[b'C'; 600]), ("2.out", &[b'D'; 600])]);
    patch_declared_sizes(&mut cumulative, 4);
    let destination = temp_dir("budget-cumulative");
    let error =
        extract_output_cases_blocking(&cumulative, &destination, 1_024).expect_err("breach");
    assert!(matches!(error, OutputArchiveError::BudgetBreached), "{error:?}");
    assert_eq!(std::fs::read(destination.join("1.out")).expect("first entry"), vec![b'C'; 600]);
    assert!(!destination.join("2.out").exists(), "the second entry must not be written");
    std::fs::remove_dir_all(destination).expect("cleanup");
}

#[test]
fn output_extraction_rejects_garbage_and_nested_paths_as_contestant_faults() {
    let destination = temp_dir("invalid-output");
    let error = extract_output_cases_blocking(b"definitely not a zip", &destination, 1_024)
        .expect_err("garbage must be rejected");
    assert!(matches!(error, OutputArchiveError::Invalid(_)), "{error:?}");

    let nested = stored_zip_bytes(&[("sub/1.out", b"42\n")]);
    let error = extract_output_cases_blocking(&nested, &destination, 1_024)
        .expect_err("nested path must be rejected");
    assert!(matches!(error, OutputArchiveError::Invalid(_)), "{error:?}");
    std::fs::remove_dir_all(destination).expect("cleanup");
}

#[test]
fn testdata_extraction_bounds_actual_bytes_lying_about_declared_sizes() {
    // The trusted testdata path keeps its InvalidTestdata classification while
    // gaining the same actual-bytes cap.
    let mut archive = stored_zip_bytes(&[("1.in", &[b'x'; 4_096]), ("1.out", &[b'y'; 8])]);
    patch_declared_sizes(&mut archive, 4);
    let work = temp_dir("testdata-budget");
    let archive_path = work.join("testdata.zip");
    std::fs::write(&archive_path, &archive).expect("archive file");
    let destination = work.join("cases");
    std::fs::create_dir_all(&destination).expect("destination");
    let error = extract_cases_blocking(&archive_path, &destination, 1_024).expect_err("breach");
    assert!(
        matches!(error, crate::sandbox::SandboxError::InvalidTestdata(ref message)
            if message.contains("expands beyond the limit")),
        "unexpected error: {error:?}"
    );
    std::fs::remove_dir_all(work).expect("cleanup");
}

#[tokio::test]
async fn invalid_output_archive_is_a_wrong_answer_not_a_system_error() {
    let work = temp_dir("output-only-judge");
    let archive_path = work.join("testdata.zip");
    std::fs::write(&archive_path, stored_zip_bytes(&[("1.in", b"1\n"), ("1.out", b"1\n")]))
        .expect("archive file");
    let job_dir = work.join("job");

    let judgement =
        run_output_only(&output_only_task(), b"this is not a zip archive", &archive_path, &job_dir)
            .await
            .expect("the outcome must be a contestant verdict, not an engine error");

    assert_eq!(judgement.verdict, JudgeVerdict::WrongAnswer);
    assert_eq!(judgement.runs.len(), 1);
    assert_eq!(judgement.runs[0].verdict, JudgeVerdict::WrongAnswer);
    let reason = judgement.runs[0].stderr_tail.as_deref().unwrap_or_default();
    assert!(reason.contains("invalid output archive"), "unexpected reason: {reason}");

    // The judgement must satisfy the result contract (a WrongAnswer with no
    // runs would be rejected by JudgeResult::validate).
    let judgement_id = uuid::Uuid::new_v4();
    let result = JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: judgement_id,
        judgement_id,
        submission_id: 42,
        worker_id: "worker-under-test".to_owned(),
        verdict: judgement.verdict,
        total_time_ms: judgement.total_time_ms,
        peak_memory_kb: judgement.peak_memory_kb,
        compile_log: judgement.compile_log,
        started_at: time::OffsetDateTime::now_utc(),
        completed_at: time::OffsetDateTime::now_utc(),
        runs: judgement.runs,
    };
    result.validate().expect("contestant outcome must be contract-valid");
    std::fs::remove_dir_all(work).expect("cleanup");
}

#[test]
fn io_errors_carry_operation_and_path_context_but_keep_their_kind() {
    use std::path::Path;

    use crate::sandbox::fs::with_path_context;

    let missing = Path::new("/nonexistent-jobs/abc/data/1.out");
    let error = with_path_context(
        std::fs::read(missing).expect_err("reading a missing path must fail"),
        "read expected output",
        missing,
    );
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(
        error.to_string().contains("read expected output /nonexistent-jobs/abc/data/1.out"),
        "context must include the operation and path, got {error}"
    );
}
