use std::io::{Cursor, Write};

use project_balloon_contracts::{JudgeMode, JudgeVerdict};
use project_balloon_judge_worker::sandbox::{DockerSandbox, DockerSandboxConfig, SandboxJudgement};
use project_balloon_test_support::valid_judge_task;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-cpp image"]
async fn cpp_submission_compiles_and_passes_in_locked_down_container() {
    let judgement = judge(
        "cpp",
        b"#include <iostream>\nint main(){ long long a,b; std::cin>>a>>b; std::cout<<a+b<<'\\n'; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
async fn c_submission_compiles_and_passes_in_locked_down_container() {
    let judgement = judge(
        "c",
        b"#include <stdio.h>\nint main(void){ long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; printf(\"%lld\\n\",a+b); return 0; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-java image"]
async fn java_submission_compiles_and_passes_in_locked_down_container() {
    let judgement = judge(
        "java",
        br#"import java.io.*;
import java.util.*;
public class Main {
    public static void main(String[] args) throws Exception {
        Scanner scanner = new Scanner(System.in);
        long a = scanner.nextLong();
        long b = scanner.nextLong();
        System.out.println(a + b);
    }
}
"#,
        256,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-python image"]
async fn python_submission_passes_in_locked_down_container() {
    let judgement = judge(
        "python",
        b"import sys\na, b = map(int, sys.stdin.buffer.read().split())\nprint(a + b)\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-cpp image"]
async fn invalid_cpp_is_compile_error() {
    let judgement = judge("cpp", b"int main( {", 128).await;
    assert_eq!(judgement.verdict, JudgeVerdict::CompileError);
    assert!(judgement.runs.is_empty());
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
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
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
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
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
async fn memory_pressure_is_memory_limit_exceeded() {
    let judgement = judge_with_limits(
        "c",
        b"#include <stdlib.h>\n#include <string.h>\nint main(void){ size_t n=256UL*1024*1024; char *p=malloc(n); if(!p) return 2; for(size_t i=0;i<n;i+=4096) p[i]=1; return p[0]; }\n",
        32,
        1_000,
        64,
    )
    .await;
    assert_eq!(judgement.verdict, JudgeVerdict::MemoryLimitExceeded);
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
async fn accepted_run_reports_peak_container_memory() {
    let judgement = judge_with_limits(
        "c",
        b"#include <stdio.h>\n#include <stdlib.h>\n#include <unistd.h>\nint main(void){ long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; char *p=malloc(8*1024*1024); if(!p) return 2; for(int i=0;i<8*1024*1024;i+=4096) p[i]=1; sleep(2); printf(\"%lld\\n\",a+b+p[0]-1); free(p); return 0; }\n",
        128,
        3_000,
        64,
    )
    .await;
    assert_accepted(&judgement);
    assert!(judgement.peak_memory_kb > 0);
    assert!(judgement.runs.iter().all(|run| run.memory_kb > 0));
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
async fn sleeping_time_is_not_charged_as_cpu_time() {
    let judgement = judge_with_limits(
        "c",
        b"#include <stdio.h>\n#include <time.h>\n#include <unistd.h>\nint main(void){ long long a,b; if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; volatile unsigned long x=0; struct timespec start,now; clock_gettime(CLOCK_MONOTONIC,&start); do { for(int i=0;i<10000;i++) x++; clock_gettime(CLOCK_MONOTONIC,&now); } while((now.tv_sec-start.tv_sec)*1000+(now.tv_nsec-start.tv_nsec)/1000000 < 100); sleep(1); printf(\"%lld\\n\",a+b+(x==0)); return 0; }\n",
        128,
        500,
        64,
    )
    .await;
    assert_accepted(&judgement);
    assert!(judgement.runs.iter().all(|run| run.time_ms < 500));
}

#[tokio::test]
#[ignore = "requires Docker and the fixed judge-runtime-c image"]
async fn expected_outputs_are_not_visible_inside_the_judgement_container() {
    let judgement = judge(
        "c",
        b"#include <stdio.h>\n#include <stdlib.h>\nint main(void){ long long a,b; if(fopen(\"/data/1.out\",\"r\") || fopen(\"/work/data/1.out\",\"r\")) { puts(\"LEAKED\"); return 0; } if(scanf(\"%lld%lld\",&a,&b)!=2) return 1; printf(\"%lld\\n\",a+b); return 0; }\n",
        128,
    )
    .await;
    assert_accepted(&judgement);
}

#[tokio::test]
#[ignore = "requires Docker runtime images"]
async fn output_only_zip_is_scored_without_executing_participant_content() {
    let root = std::env::temp_dir().join(format!("project-balloon-output-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("test data");
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.judge_mode = JudgeMode::OutputOnly;
    task.language = "output".to_owned();
    let sandbox = test_sandbox(root.clone());
    sandbox.preflight().await.expect("preflight");
    let judgement =
        sandbox.judge(&task, &output_archive(), &archive, None).await.expect("judge output");
    assert_accepted(&judgement);
    tokio::fs::remove_dir_all(root).await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires Docker, cc, and the fixed C runtime image"]
async fn interactive_program_and_interactor_exchange_over_pipes() {
    let root = std::env::temp_dir().join(format!("project-balloon-interactive-{}", Uuid::new_v4()));
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
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = "c".to_owned();
    task.judge_mode = JudgeMode::Interactive;
    task.interactor_object_key = Some("problems/7/interactor".to_owned());
    task.interactor_sha256 = Some("a".repeat(64));
    let source = b"#include <stdio.h>\nint main(void){long long a,b;if(scanf(\"%lld%lld\",&a,&b)!=2)return 1;printf(\"%lld\\n\",a+b);fflush(stdout);return 0;}\n";
    let sandbox = test_sandbox(root.clone());
    sandbox.preflight().await.expect("preflight");
    let judgement =
        sandbox.judge(&task, source, &archive, Some(&interactor)).await.expect("interactive judge");
    assert_accepted(&judgement);
    tokio::fs::remove_dir_all(root).await.expect("cleanup");
}

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
    let root = std::env::temp_dir().join(format!("project-balloon-sandbox-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.expect("create sandbox test root");
    let archive = root.join("testdata.zip");
    tokio::fs::write(&archive, fixture_archive()).await.expect("write test-data archive");
    let mut task = valid_judge_task();
    task.judgement_id = Uuid::new_v4();
    task.language = language.to_owned();
    task.time_limit_ms = time_limit_ms;
    task.memory_limit_mb = memory_limit_mb;
    task.output_limit_kb = output_limit_kb;
    let sandbox = test_sandbox(root.clone());
    sandbox.preflight().await.expect("sandbox preflight");

    let judgement = sandbox.judge(&task, source, &archive, None).await.expect("execute submission");
    tokio::fs::remove_dir_all(root).await.expect("remove sandbox test root");
    judgement
}

fn test_sandbox(root: std::path::PathBuf) -> DockerSandbox {
    DockerSandbox::connect(DockerSandboxConfig {
        socket: "/var/run/docker.sock".into(),
        cache_dir: root.clone(),
        runtime: None,
        user: std::env::var("PROJECT_BALLOON_TEST_SANDBOX_USER")
            .unwrap_or_else(|_| "1000:1000".to_owned()),
        c_image: "judge-runtime-c:12.2.0".to_owned(),
        cpp_image: "judge-runtime-cpp:12.2.0".to_owned(),
        java_image: "judge-runtime-java:21".to_owned(),
        python_image: "judge-runtime-python:3.12.13".to_owned(),
    })
    .expect("connect sandbox client")
}

fn assert_accepted(judgement: &SandboxJudgement) {
    assert_eq!(judgement.verdict, JudgeVerdict::Accepted);
    assert_eq!(judgement.runs.len(), 2);
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
