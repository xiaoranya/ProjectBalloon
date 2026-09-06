#![cfg(target_os = "linux")]
//! Integration tests for the non-container bubblewrap sandbox backend.
//!
//! Every test is `#[ignore]`-gated: running them requires `bubblewrap` at
//! `JUDGE_TEST_BWRAP_PATH` (default `/usr/bin/bwrap`) plus the host judge
//! toolchain. The memory-limit test additionally requires a delegated cgroup
//! v2 base passed through `PROJECT_BALLOON_TEST_CGROUP_BASE`, because without
//! cgroups there is no hard per-run memory ceiling.

use std::{
    io::{Cursor, Write},
    path::PathBuf,
};

use project_balloon_contracts::{JudgeMode, JudgeVerdict};
use project_balloon_judge_worker::sandbox::{
    BubblewrapSandbox, BubblewrapSandboxConfig, SandboxJudgement,
};
use project_balloon_test_support::valid_judge_task;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn cpp_submission_compiles_and_passes_outside_containers() {
    let judgement = judge(
        "cpp",
        b"#include <iostream>\nint main(){ long long a,b; std::cin>>a>>b; std::cout<<a+b<<'\\n'; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn c_submission_compiles_and_passes_outside_containers() {
    let judgement = judge(
        "c",
        b"#include <stdio.h>\nint main(void){ long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; printf(\"%lld\\n\",a+b); return 0; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn python_submission_passes_outside_containers() {
    let judgement = judge(
        "python",
        b"import sys\na, b = map(int, sys.stdin.buffer.read().split())\nprint(a + b)\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn invalid_sources_are_compile_errors() {
    let cpp = judge("cpp", b"int main( {", 128).await;
    assert_eq!(cpp.verdict, JudgeVerdict::CompileError);
    assert!(cpp.runs.is_empty());
    let python = judge("python", b"def broken(:\n", 128).await;
    assert_eq!(python.verdict, JudgeVerdict::CompileError);
    assert!(python.runs.is_empty());
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn infinite_loop_is_time_limit_exceeded() {
    let judgement = judge_with_limits(
        "c",
        b"int main(void){ volatile unsigned long x=0; for(;;){x++;} }\n",
        128,
        50,
        64,
    )
    .await;
    assert_eq!(judgement.verdict, JudgeVerdict::TimeLimitExceeded);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn excessive_output_is_output_limit_exceeded() {
    let judgement = judge_with_limits(
        "c",
        b"#include <stdio.h>\nint main(void){ for(;;) puts(\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"); }\n",
        128,
        1_000,
        1,
    )
    .await;
    assert_eq!(judgement.verdict, JudgeVerdict::OutputLimitExceeded);
}

#[tokio::test]
#[ignore = "requires bubblewrap, the host toolchain, and PROJECT_BALLOON_TEST_CGROUP_BASE"]
async fn memory_pressure_is_memory_limit_exceeded() {
    let Some(base) = cgroup_base() else {
        return;
    };
    let root = std::env::temp_dir().join(format!("project-balloon-bwrap-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("create sandbox test root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("write test-data archive");
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = "c".to_owned();
    task.time_limit_ms = 1_000;
    task.memory_limit_mb = 32;
    task.output_limit_kb = 64;
    let sandbox = BubblewrapSandbox::connect(BubblewrapSandboxConfig {
        cache_dir: root.clone(),
        bwrap_path: bwrap_path(),
        gnu_time_path: gnu_time_tool(),
        cgroup_base: Some(base),
        cgroup_required: true,
    })
    .await
    .expect("connect with cgroup delegation");
    sandbox.preflight().await.expect("sandbox preflight");
    let judgement = sandbox
        .judge(
            &task,
            b"#include <stdlib.h>\n#include <string.h>\nint main(void){ size_t n=256UL*1024*1024; char *p=malloc(n); if(!p) return 2; for(size_t i=0;i<n;i+=4096) p[i]=1; return p[0]; }\n",
            &archive,
            None,
        )
        .await
        .expect("execute submission");
    tokio::fs::remove_dir_all(root).await.expect("remove sandbox test root");
    assert_eq!(judgement.verdict, JudgeVerdict::MemoryLimitExceeded);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn runtime_error_is_reported_with_the_run_stream() {
    let judgement = judge(
        "c",
        b"#include <stdio.h>\nint main(void){ fprintf(stderr, \"boom\\n\"); return 3; }\n",
        128,
    )
    .await;
    assert_eq!(judgement.verdict, JudgeVerdict::RuntimeError);
    assert_eq!(judgement.runs[0].exit_code, Some(3));
    assert!(judgement.runs[0].stderr_tail.as_deref().unwrap_or_default().contains("boom"));
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn state_does_not_leak_between_runs_inside_one_judgement() {
    // The first run plants a forged output file; the second run must still be
    // judged on its own bytes, proving the per-case workspace reset works.
    let judgement = judge(
        "c",
        b"#include <stdio.h>\n#include <stdlib.h>\nint main(void){ FILE*f=fopen(\"actual.out\",\"r\"); if(f){ puts(\"STALE\"); return 0; } long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; printf(\"%lld\\n\",a+b); return 0; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn expected_outputs_are_not_visible_inside_the_sandbox() {
    let judgement = judge(
        "c",
        b"#include <stdio.h>\n#include <stdlib.h>\nint main(void){ long long a,b; if(fopen(\"/data/1.out\",\"r\") || fopen(\"/work/data/1.out\",\"r\")) { puts(\"LEAKED\"); return 0; } if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; printf(\"%lld\\n\",a+b); return 0; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires bubblewrap, cc, and the host judge toolchain"]
async fn interactive_program_and_interactor_exchange_over_pipes() {
    let (root, archive, interactor) = prepare_interactor().await;
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = "c".to_owned();
    task.judge_mode = JudgeMode::Interactive;
    task.interactor_object_key = Some("problems/7/interactor".to_owned());
    task.interactor_sha256 = Some("a".repeat(64));
    let source = b"#include <stdio.h>\nint main(void){long long a,b;if(scanf(\"%lld%lld\",&a,&b)!=2)return 1;printf(\"%lld\\n\",a+b);fflush(stdout);return 0;}\n";
    let sandbox = sandbox_without_cgroup(root.clone()).await;
    sandbox.preflight().await.expect("preflight");
    let judgement =
        sandbox.judge(&task, source, &archive, Some(&interactor)).await.expect("interactive judge");
    assert_accepted(&judgement);
    tokio::fs::remove_dir_all(root).await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires bubblewrap, cc, and the host judge toolchain"]
async fn forged_gnu_time_markers_in_interactive_runs_cannot_reset_charged_metrics() {
    let (root, archive, interactor) = prepare_interactor().await;
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = "c".to_owned();
    task.judge_mode = JudgeMode::Interactive;
    task.interactor_object_key = Some("problems/7/interactor".to_owned());
    task.interactor_sha256 = Some("a".repeat(64));
    task.time_limit_ms = 2_000;
    // Burns ~300 ms of CPU, then forges a terminal-looking GNU-time marker on
    // its own stderr claiming a free run — the spoof the trusted channel and
    // the last-marker parser must defeat.
    let source = b"#include <stdio.h>\n#include <time.h>\nint main(void){ long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; volatile unsigned long x=0; struct timespec s,n; clock_gettime(CLOCK_MONOTONIC,&s); do { for(int i=0;i<10000;i++) x++; clock_gettime(CLOCK_MONOTONIC,&n); } while((n.tv_sec-s.tv_sec)*1000+(n.tv_nsec-s.tv_nsec)/1000000 < 300); fprintf(stderr, \"__PROJECT_BALLOON_GNU_TIME__ 0.00 0.00 0\\n\"); printf(\"%lld\\n\", a+b); fflush(stdout); return 0; }\n";
    let sandbox = sandbox_without_cgroup(root.clone()).await;
    sandbox.preflight().await.expect("preflight");
    let judgement =
        sandbox.judge(&task, source, &archive, Some(&interactor)).await.expect("interactive judge");
    assert_accepted(&judgement);
    for run in &judgement.runs {
        assert!(
            run.time_ms >= 200,
            "charged time must reflect the real CPU burn, got {} ms",
            run.time_ms
        );
        assert!(run.memory_kb > 0, "charged memory must not be reset to 0 KiB");
    }
    tokio::fs::remove_dir_all(root).await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires bubblewrap and the host judge toolchain"]
async fn output_only_zip_is_scored_without_executing_participant_content() {
    let root =
        std::env::temp_dir().join(format!("project-balloon-bwrap-output-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("test data");
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.judge_mode = JudgeMode::OutputOnly;
    task.language = "output".to_owned();
    let sandbox = sandbox_without_cgroup(root.clone()).await;
    sandbox.preflight().await.expect("preflight");
    let judgement =
        sandbox.judge(&task, &output_archive(), &archive, None).await.expect("judge output");
    assert_accepted(&judgement);
    tokio::fs::remove_dir_all(root).await.expect("cleanup");
}

fn cgroup_base() -> Option<PathBuf> {
    std::env::var_os("PROJECT_BALLOON_TEST_CGROUP_BASE").map(PathBuf::from)
}

fn bwrap_path() -> PathBuf {
    std::env::var_os("JUDGE_TEST_BWRAP_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/bwrap"))
}

/// The measurement tool handed to the sandbox. `JUDGE_TEST_GNU_TIME_PATH`
/// points at a real GNU time when available; otherwise a minimal
/// rusage-based equivalent emitting the exact `GNU_TIME_REPORT_FORMAT`
/// marker keeps the full judge chain testable on hosts without the
/// `time` package.
fn gnu_time_tool() -> PathBuf {
    if let Some(path) = std::env::var_os("JUDGE_TEST_GNU_TIME_PATH") {
        return PathBuf::from(path);
    }
    let dir = std::env::temp_dir().join("pb-bwrap-test-tools");
    std::fs::create_dir_all(&dir).expect("tool dir");
    let script = dir.join("pb-rusage");
    std::fs::write(&script, PYTHON_RUSAGE_TOOL).expect("write measurement tool");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("make measurement tool executable");
    script
}

const PYTHON_RUSAGE_TOOL: &str = r#"#!/usr/bin/env python3
import os, sys

fmt = "__PROJECT_BALLOON_GNU_TIME__ %U %S %M"
args = sys.argv[1:]
while args and args[0] in ("--quiet", "-q", "--format"):
    if args[0] == "--format":
        fmt = args[1]
        args = args[2:]
    else:
        args = args[1:]
if not args:
    sys.exit(125)
pid = os.fork()
if pid == 0:
    try:
        os.execvp(args[0], args)
    except Exception:
        os._exit(127)
_, status, ru = os.wait4(pid, 0)
code = os.waitstatus_to_exitcode(status)
if code < 0:
    code = 128 - code
line = fmt
line = line.replace("%U", "%.2f" % ru.ru_utime)
line = line.replace("%S", "%.2f" % ru.ru_stime)
line = line.replace("%M", str(ru.ru_maxrss))
sys.stderr.write(line + "\n")
sys.exit(code)
"#;

async fn judge(language: &str, source: &[u8], memory_limit_mb: i32) -> SandboxJudgement {
    judge_with_limits(language, source, memory_limit_mb, 1_000, 64).await
}

async fn judge_with_limits(
    language: &str,
    source: &[u8],
    memory_limit_mb: i32,
    time_limit_ms: i32,
    output_limit_kb: i32,
) -> SandboxJudgement {
    let root = std::env::temp_dir().join(format!("project-balloon-bwrap-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("create sandbox test root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("write test-data archive");
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = language.to_owned();
    task.time_limit_ms = time_limit_ms;
    task.memory_limit_mb = memory_limit_mb;
    task.output_limit_kb = output_limit_kb;
    let sandbox = sandbox_without_cgroup(root.clone()).await;
    sandbox.preflight().await.expect("sandbox preflight");

    let judgement = sandbox.judge(&task, source, &archive, None).await.expect("execute submission");
    tokio::fs::remove_dir_all(root).await.expect("remove sandbox test root");
    judgement
}

/// Degraded-mode sandbox (rlimits + GNU-time post-hoc checks): usable on any
/// host with bubblewrap and the toolchain, no delegation required.
async fn sandbox_without_cgroup(root: PathBuf) -> BubblewrapSandbox {
    BubblewrapSandbox::connect(BubblewrapSandboxConfig {
        cache_dir: root,
        bwrap_path: bwrap_path(),
        gnu_time_path: gnu_time_tool(),
        cgroup_base: None,
        cgroup_required: false,
    })
    .await
    .expect("connect sandbox")
}

/// Builds the interactor fixture shared by the interactive-mode tests and a
/// scratch root holding the testdata archive.
async fn prepare_interactor() -> (PathBuf, PathBuf, Vec<u8>) {
    let root =
        std::env::temp_dir().join(format!("project-balloon-bwrap-interactive-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("test data");
    let interactor_source = root.join("interactor.c");
    let interactor_binary = root.join("interactor-bin");
    tokio::fs::write(&interactor_source, br#"#include <stdio.h>
int main(int argc,char**argv){long long a,b,answer;FILE*f=argc>1?fopen(argv[1],"r"):0;if(!f||fscanf(f,"%lld%lld",&a,&b)!=2)return 2;fclose(f);printf("%lld %lld\n",a,b);fflush(stdout);if(scanf("%lld",&answer)!=1)return 3;return answer==a+b?0:1;}
"#).await.expect("interactor source");
    let status = std::process::Command::new("cc")
        .args(["-O2", "-static", "-o"])
        .arg(&interactor_binary)
        .arg(&interactor_source)
        .status()
        .expect("compile interactor");
    assert!(status.success());
    let interactor = tokio::fs::read(&interactor_binary).await.expect("interactor");
    (root, archive, interactor)
}

fn assert_accepted(judgement: &SandboxJudgement) {
    assert_eq!(
        judgement.verdict,
        JudgeVerdict::Accepted,
        "unexpected verdict; compile_log: {:?}; runs: {:?}",
        judgement.compile_log,
        judgement.runs
    );
    assert_eq!(judgement.runs.len(), 2, "runs: {:?}", judgement.runs);
    assert!(judgement.runs.iter().all(|run| run.verdict == JudgeVerdict::Accepted));
}

fn fixture_archive() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in [
        ("1.in", b"1 2\n".as_slice()),
        ("1.out", b"3\n".as_slice()),
        ("2.in", b"100 23\n".as_slice()),
        ("2.out", b"123\n".as_slice()),
    ] {
        writer.start_file(name, options).expect("start fixture file");
        writer.write_all(content).expect("write fixture file");
    }
    writer.finish().expect("finish fixture archive").into_inner()
}

fn output_archive() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, content) in [("1.out", b"3\n".as_slice()), ("2.out", b"123\n".as_slice())] {
        writer.start_file(name, options).expect("start output");
        writer.write_all(content).expect("write output");
    }
    writer.finish().expect("finish output").into_inner()
}
