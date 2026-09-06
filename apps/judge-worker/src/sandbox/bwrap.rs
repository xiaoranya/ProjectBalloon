//! Bubblewrap (non-container) sandbox backend for Linux.
//!
//! Every action — one compile, or one test-case run — is a single fresh
//! `bwrap` process: unshared user/network/pid/mount namespaces, a read-only
//! bind of the host toolchain directories, one writable `/work` bind, an
//! rlimit-shaped cgroup v2 limit set, and a clean environment. Isolation is
//! self-cleaning: `bwrap` is the pid-namespace init, so when it exits (or is
//! killed after the wall-clock deadline) the kernel tears down every
//! contestant process with it — there is no leftover container and no residue
//! between actions.
//!
//! Judgement semantics (verdict order, GNU-time marker parsing, wall-clock
//! multipliers, output comparisons) are shared with the container backend:
//! [`resource_verdict`], [`extract_gnu_time_metrics`], [`standard_shell`], and
//! [`interactive_shell`] behave identically here.

use std::{
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use project_balloon_contracts::{JudgeMode, JudgeRunResult, JudgeTask, JudgeVerdict};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
use tracing::warn;
use uuid::Uuid;

use crate::sandbox::{
    COMPILE_WALL_LIMIT, MAX_EXEC_LOG_BYTES, SandboxError, SandboxJudgement,
    cgroup::CgroupManager,
    effective_time_limit,
    fs::{
        create_private_dir, nonempty, read_regular_output_no_follow, remove_dir_if_present,
        remove_file_if_present, set_executable_file_permissions, set_private_file_permissions,
        truncate_log, with_path_context,
    },
    language::LanguageConfig,
    metrics::extract_gnu_time_metrics,
    run_wall_limit,
    runner::{interactive_shell, resource_verdict, run_output_only, standard_shell},
};

/// Host directories bound read-only into every sandbox. Merged-usr layouts
/// (`/bin -> /usr/bin` and friends) are fine: bwrap binds the resolved target.
const HOST_BIND_CANDIDATES: [&str; 6] = ["/usr", "/etc", "/bin", "/sbin", "/lib", "/lib64"];

/// Toolchain binaries the host must provide for every judge language.
const TOOLCHAIN_BINARIES: [&str; 7] = ["gcc", "g++", "javac", "python3", "go", "rustc", "sh"];

/// GNU time is invoked by this fixed path inside the run shells; the host
/// tool is bound there read-only (below the writable /work mount).
const SANDBOX_TIME_TOOL: &str = "/work/.pb-time";

/// `PATH` set inside the sandbox after `--clearenv`.
const SANDBOX_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Bound for the one-shot probes (`bwrap --version`, namespace smoke test).
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Memory floor for the compile cgroup, mirroring the container backend's
/// create-time `memory = max(run limit, 1 GiB)` (compilers need more than a
/// typical task memory limit).
const COMPILE_MEMORY_FLOOR_BYTES: i64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BubblewrapSandboxConfig {
    pub cache_dir: PathBuf,
    pub bwrap_path: PathBuf,
    /// Host path of the GNU-time-compatible measurement tool. It is bound into
    /// every sandbox at the fixed `/usr/bin/time` location the shared run
    /// shells invoke.
    pub gnu_time_path: PathBuf,
    /// Delegated cgroup v2 base directory. Required unless
    /// `cgroup_required` is `false`.
    pub cgroup_base: Option<PathBuf>,
    /// Fail startup when cgroup v2 is unavailable instead of degrading to
    /// rlimit-only enforcement.
    pub cgroup_required: bool,
}

#[derive(Clone)]
pub struct BubblewrapSandbox {
    cache_dir: PathBuf,
    bwrap_path: PathBuf,
    gnu_time_path: PathBuf,
    /// `None` means degraded mode: no cgroup limits (rlimits and GNU-time
    /// post-hoc checks only). Never `None` when `cgroup_required` was set.
    cgroup: Option<CgroupManager>,
    host_binds: Vec<&'static str>,
}

/// One sandbox action's observable outcome, shaped like the container
/// backend's `ContainerRun` so the verdict logic stays aligned.
struct BwrapRun {
    exit_code: i64,
    timed_out: bool,
    oom_killed: bool,
    elapsed_ms: i32,
    logs: String,
}

impl BubblewrapSandbox {
    /// Prepares the backend. The cgroup probe decides strict enforcement
    /// versus a logged degradation, mirroring the container backend's
    /// connect-time socket handshake.
    pub async fn connect(config: BubblewrapSandboxConfig) -> Result<Self, SandboxError> {
        let cgroup = match config.cgroup_base {
            Some(base) => {
                let manager = CgroupManager::new(base);
                match manager.probe().await {
                    Ok(()) => Some(manager),
                    Err(reason) if config.cgroup_required => {
                        return Err(SandboxError::Api(reason));
                    }
                    Err(reason) => {
                        warn!(
                            reason = %reason,
                            "cgroup v2 probe failed; the bubblewrap backend degrades to rlimit-only enforcement"
                        );
                        None
                    }
                }
            }
            None if config.cgroup_required => {
                return Err(SandboxError::Api(
                    "JUDGE_CGROUP_BASE must point at a delegated cgroup v2 directory for the \
                     bubblewrap backend (or set JUDGE_CGROUP_REQUIRED=false to accept degraded \
                     enforcement)"
                        .to_owned(),
                ));
            }
            None => {
                warn!(
                    "no cgroup base configured; the bubblewrap backend degrades to rlimit-only enforcement"
                );
                None
            }
        };
        let host_binds = HOST_BIND_CANDIDATES
            .into_iter()
            .filter(|directory| Path::new(directory).symlink_metadata().is_ok())
            .collect();
        Ok(Self {
            cache_dir: config.cache_dir,
            bwrap_path: config.bwrap_path,
            gnu_time_path: config.gnu_time_path,
            cgroup,
            host_binds,
        })
    }

    pub async fn preflight(&self) -> Result<(), SandboxError> {
        let version = tokio::time::timeout(
            PROBE_TIMEOUT,
            Command::new(&self.bwrap_path).arg("--version").output(),
        )
        .await;
        match version {
            Ok(Ok(output)) if output.status.success() => {}
            Ok(Ok(output)) => {
                return Err(SandboxError::Api(format!(
                    "bubblewrap at {} is not runnable (exit {:?})",
                    self.bwrap_path.display(),
                    output.status.code()
                )));
            }
            Ok(Err(error)) => {
                return Err(SandboxError::Api(format!(
                    "cannot execute bubblewrap at {}: {error}",
                    self.bwrap_path.display()
                )));
            }
            Err(_) => {
                return Err(SandboxError::Api("timed out running bubblewrap --version".to_owned()));
            }
        }

        // Smoke-test the actual privilege boundary: unprivileged user
        // namespaces are the foundation of this backend, and distributions
        // disable them in different ways (sysctl, AppArmor, seccomp profiles).
        let smoke = tokio::time::timeout(
            PROBE_TIMEOUT,
            Command::new(&self.bwrap_path)
                .args(["--unshare-all", "--ro-bind", "/", "/", "--", "/bin/echo", "pb-bwrap-ok"])
                .stdin(Stdio::null())
                .output(),
        )
        .await;
        match smoke {
            Ok(Ok(output))
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == "pb-bwrap-ok" => {}
            Ok(Ok(output)) => {
                return Err(SandboxError::Api(format!(
                    "bubblewrap cannot create its namespaces (exit {:?}, stderr: {}); check \
                     unprivileged user namespace availability (e.g. \
                     kernel.unprivileged_userns_clone, AppArmor user_namespace creation \
                     restrictions)",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            Ok(Err(error)) => {
                return Err(SandboxError::Api(format!(
                    "bubblewrap namespace smoke test failed: {error}"
                )));
            }
            Err(_) => {
                return Err(SandboxError::Api(
                    "timed out running the bubblewrap smoke test".to_owned(),
                ));
            }
        }

        for tool in TOOLCHAIN_BINARIES {
            if resolve_in_path(tool).is_none() {
                return Err(SandboxError::Api(format!(
                    "host toolchain binary {tool} not found on PATH (the bubblewrap backend \
                     uses the host toolchain)"
                )));
            }
        }
        if !self.gnu_time_path.is_file() {
            return Err(SandboxError::Api(format!(
                "GNU time tool not found at {} (the bubblewrap backend measures runs with the \
                 host tool; configure JUDGE_GNU_TIME_PATH if it lives elsewhere)",
                self.gnu_time_path.display()
            )));
        }
        if self.cgroup.is_none() {
            warn!(
                "preflight continues without cgroup v2: memory/pids/cpu limits are enforced by \
                 rlimits and post-hoc GNU-time checks only"
            );
        }
        Ok(())
    }

    pub async fn judge(
        &self,
        task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        interactor: Option<&[u8]>,
    ) -> Result<SandboxJudgement, SandboxError> {
        let job_dir = self.cache_dir.join("jobs").join(task.judgement_id.to_string());
        remove_dir_if_present(&job_dir).await?;
        create_private_dir(&job_dir).await?;
        let result = self.judge_in_dir(task, source, archive, interactor, &job_dir).await;
        let cleanup = remove_dir_if_present(&job_dir).await;
        match (result, cleanup) {
            (Ok(judgement), Ok(())) => Ok(judgement),
            (Err(error), _) => Err(error),
            // A finished judgement is never re-run over cleanup trouble: the
            // leftover directory is reclaimed by the orphan sweeper.
            (Ok(judgement), Err(error)) => {
                warn!(
                    judgement_id = %task.judgement_id,
                    error = %error,
                    "job directory cleanup failed after a completed judgement; keeping the result"
                );
                Ok(judgement)
            }
        }
    }

    async fn judge_in_dir(
        &self,
        task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        interactor: Option<&[u8]>,
        job_dir: &Path,
    ) -> Result<SandboxJudgement, SandboxError> {
        if task.judge_mode == JudgeMode::OutputOnly {
            return run_output_only(task, source, archive, job_dir).await;
        }
        let language = LanguageConfig::for_task(task)?;
        let work_dir = job_dir.join("work");
        create_private_dir(&work_dir).await?;
        let source_path = work_dir.join(language.source_filename());
        tokio::fs::write(&source_path, source)
            .await
            .map_err(|error| with_path_context(error, "write submission source", &source_path))?;
        set_private_file_permissions(&source_path).await?;
        if let Some(interactor) = interactor {
            let path = work_dir.join("interactor");
            tokio::fs::write(&path, interactor)
                .await
                .map_err(|error| with_path_context(error, "write interactor", &path))?;
            set_executable_file_permissions(&path).await?;
        }
        let data_dir = job_dir.join("data");
        create_private_dir(&data_dir).await?;
        let case_count =
            crate::sandbox::archive::extract_cases(archive.to_owned(), data_dir.clone()).await?;

        let run_memory_bytes = i64::from(task.memory_limit_mb) * 1024 * 1024;
        let compile = self
            .run_command(
                &format!("{}-compile", task.judgement_id),
                &language.compile_command(),
                &language.compile_env(),
                &work_dir,
                run_memory_bytes.max(COMPILE_MEMORY_FLOOR_BYTES),
                COMPILE_WALL_LIMIT,
            )
            .await?;
        let compile_log = truncate_log(&compile.logs, 64 * 1024);
        if compile.timed_out || compile.exit_code != 0 {
            return Ok(SandboxJudgement {
                verdict: JudgeVerdict::CompileError,
                total_time_ms: compile.elapsed_ms,
                peak_memory_kb: 0,
                compile_log: Some(compile_log),
                runs: Vec::new(),
            });
        }

        let mut runs = Vec::with_capacity(case_count);
        let mut total_time_ms = 0_i32;
        let effective_time_limit_ms =
            effective_time_limit(task.time_limit_ms, task.language_multiplier);
        let wall_limit = run_wall_limit(effective_time_limit_ms);
        for test_index in 1..=case_count {
            let input_path = work_dir.join("current.in");
            tokio::fs::copy(data_dir.join(format!("{test_index}.in")), &input_path)
                .await
                .map_err(|error| with_path_context(error, "copy test-case input", &input_path))?;
            set_private_file_permissions(&input_path).await?;
            let actual_path = work_dir.join("actual.out");
            remove_file_if_present(&actual_path).await?;
            let output_blocks = output_file_blocks(task.output_limit_kb);
            let program = language.run_command(task.memory_limit_mb);
            let shell = if task.judge_mode == JudgeMode::Interactive {
                interactive_shell(SANDBOX_TIME_TOOL, &program, output_blocks)
            } else {
                standard_shell(SANDBOX_TIME_TOOL, &program, output_blocks)
            };
            let mut run = self
                .run_command(
                    &format!("{}-run", task.judgement_id),
                    &["/bin/sh".to_owned(), "-c".to_owned(), shell],
                    &[],
                    &work_dir,
                    run_memory_bytes,
                    wall_limit,
                )
                .await?;
            // Contestant-writable diagnostic files are read from the host side
            // only AFTER the GNU-time marker has been parsed: the parser trusts
            // the last marker, so forged bytes must never be able to follow
            // the real report in the stream it parses.
            let diagnostics = if task.judge_mode == JudgeMode::Interactive {
                read_interactive_diagnostics(&work_dir).await
            } else {
                String::new()
            };
            let (sanitized_logs, gnu_time) = extract_gnu_time_metrics(&run.logs);
            run.logs = sanitized_logs;
            if !diagnostics.is_empty() {
                run.logs.push_str(&diagnostics);
            }
            if gnu_time.is_none() && !run.timed_out && !run.oom_killed && run.exit_code != 137 {
                return Err(SandboxError::Api(format!(
                    "GNU time did not produce resource metrics for a completed run (exit_code={}, stderr={:?})",
                    run.exit_code, run.logs
                )));
            }
            let charged_time_ms = gnu_time.map_or(run.elapsed_ms, |metrics| metrics.cpu_time_ms);
            // This backend has no docker-stats memory stream: the GNU-time
            // report is the only peak-memory source, and OOM runs report the
            // cgroup verdict instead of a memory number.
            let peak_memory_kb = gnu_time.map_or(0, |metrics| metrics.peak_memory_kb);
            total_time_ms = total_time_ms.saturating_add(charged_time_ms);
            // Keep the opened descriptor for the later comparison. A path check followed
            // by a separate read is racy: a surviving child can replace the path after the
            // check. O_NOFOLLOW also prevents a host-side symlink escape.
            let output = tokio::task::spawn_blocking({
                let actual_path = actual_path.clone();
                move || read_regular_output_no_follow(&actual_path)
            })
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))??;
            let output_bytes = output.as_ref().map_or(0, |output| output.len() as u64);
            let output_limit_bytes = u64::try_from(task.output_limit_kb).unwrap_or(0) * 1024;
            let resource = resource_verdict(
                run.oom_killed,
                run.exit_code,
                run.timed_out,
                output_bytes,
                output_limit_bytes,
                charged_time_ms,
                effective_time_limit_ms,
            );
            let verdict = if let Some(verdict) = resource {
                verdict
            } else if task.judge_mode == JudgeMode::Interactive && run.exit_code == 20 {
                JudgeVerdict::WrongAnswer
            } else if run.exit_code != 0 || output.is_none() {
                JudgeVerdict::RuntimeError
            } else if task.judge_mode == JudgeMode::Interactive {
                JudgeVerdict::Accepted
            } else {
                let expected_path = data_dir.join(format!("{test_index}.out"));
                let expected = tokio::fs::read(&expected_path).await.map_err(|error| {
                    with_path_context(error, "read expected output", &expected_path)
                })?;
                if crate::sandbox::compare::standard_output_matches(
                    &expected,
                    output.as_deref().unwrap_or_default(),
                ) {
                    JudgeVerdict::Accepted
                } else {
                    JudgeVerdict::WrongAnswer
                }
            };
            runs.push(JudgeRunResult {
                test_index: i32::try_from(test_index).unwrap_or(i32::MAX),
                verdict,
                time_ms: charged_time_ms,
                memory_kb: peak_memory_kb,
                exit_code: i32::try_from(run.exit_code).ok(),
                stderr_tail: nonempty(truncate_log(&run.logs, 16 * 1024)),
            });
            if verdict != JudgeVerdict::Accepted {
                break;
            }
        }
        let verdict = runs
            .iter()
            .find(|run| run.verdict != JudgeVerdict::Accepted)
            .map_or(JudgeVerdict::Accepted, |run| run.verdict);
        Ok(SandboxJudgement {
            verdict,
            total_time_ms,
            peak_memory_kb: runs.iter().map(|run| run.memory_kb).max().unwrap_or(0),
            compile_log: nonempty(compile_log),
            runs,
        })
    }

    /// Runs one sandbox action: `bwrap <isolation> --bind work /work -- <argv>`
    /// under a fresh cgroup, bounded by the wall clock. Returns the exit code,
    /// timeout flag, OOM flag, and the bounded stderr stream.
    async fn run_command(
        &self,
        label: &str,
        argv: &[String],
        env: &[String],
        work_dir: &Path,
        memory_bytes: i64,
        wall_limit: Duration,
    ) -> Result<BwrapRun, SandboxError> {
        let guard = match &self.cgroup {
            Some(manager) => Some(
                manager
                    .create(label, memory_bytes)
                    .await
                    .map_err(|error| SandboxError::Api(error.to_string()))?,
            ),
            None => None,
        };
        let mut args = self.base_args();
        for entry in env {
            let Some((name, value)) = entry.split_once('=') else {
                return Err(SandboxError::Api(format!(
                    "sandbox env entry is not KEY=VALUE: {entry}"
                )));
            };
            args.push("--setenv".into());
            args.push(name.into());
            args.push(value.into());
        }
        // The measurement tool is bound read-only INTO the writable /work
        // mount, after /work itself is mounted: bwrap can then create the
        // mount point on the writable layer even when the tool lives outside
        // the read-only host binds. Being a read-only mount, the contestant
        // can neither replace nor remove it.
        args.push("--bind".into());
        args.push(work_dir.as_os_str().to_owned());
        args.push("/work".into());
        args.push("--ro-bind".into());
        args.push(self.gnu_time_path.as_os_str().to_owned());
        args.push(SANDBOX_TIME_TOOL.into());
        args.push("--".into());
        args.extend(argv.iter().map(Into::into));

        let started = tokio::time::Instant::now();
        let mut command = Command::new(&self.bwrap_path);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // The in-sandbox environment is built entirely by --setenv.
            .env_clear()
            // Reap-orphan safety: the child is killed if this future is dropped.
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child =
            command.spawn().map_err(|error| SandboxError::Api(format!("spawn bwrap: {error}")))?;
        let child_pid = child.id();
        if let Some(guard) = &guard {
            let pid = child_pid
                .ok_or_else(|| SandboxError::Api("bwrap exited before cgroup attach".to_owned()))?;
            if let Err(error) = guard.attach(pid).await {
                // The unbounded child must not survive a failed confinement;
                // a kill error means it already exited on its own.
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(SandboxError::Api(format!("attach bwrap to sandbox cgroup: {error}")));
            }
        }
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| SandboxError::Api("bwrap stderr pipe unavailable".to_owned()))?;
        let reader = tokio::spawn(async move { collect_bounded_stream(&mut stderr).await });

        let deadline = tokio::time::Instant::now() + wall_limit;
        let mut timed_out = false;
        let wait_result = tokio::select! {
            result = child.wait() => {
                Some(result.map_err(|error| SandboxError::Api(format!("wait bwrap: {error}")))?)
            }
            _ = tokio::time::sleep_until(deadline) => {
                timed_out = true;
                None
            }
        };
        // bwrap is the pid-namespace init: SIGKILL tears down every contestant
        // process. On the happy path this is a no-op (bwrap already exited);
        // on timeouts and straggler children it is the whole cleanup. A kill
        // error means bwrap is already gone, which is the same outcome.
        let _ = child.start_kill();
        let status = match wait_result {
            Some(status) => status,
            None => child
                .wait()
                .await
                .map_err(|error| SandboxError::Api(format!("reap bwrap: {error}")))?,
        };
        let logs = match tokio::time::timeout(Duration::from_secs(2), reader).await {
            Ok(Ok(logs)) => logs,
            _ => String::new(),
        };
        let oom_killed = match &guard {
            Some(guard) => guard.oom_kill_count().await.unwrap_or(0) > 0,
            None => false,
        };
        if let Some(guard) = guard {
            guard.release().await;
        }
        Ok(BwrapRun {
            exit_code: exit_code_of(&status),
            timed_out,
            oom_killed,
            elapsed_ms: i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX),
            logs,
        })
    }

    /// The isolation skeleton every action shares.
    fn base_args(&self) -> Vec<std::ffi::OsString> {
        let mut args: Vec<std::ffi::OsString> = vec![
            "--unshare-all".into(),
            "--die-with-parent".into(),
            "--new-session".into(),
            "--cap-drop".into(),
            "ALL".into(),
            "--clearenv".into(),
            "--dev".into(),
            "/dev".into(),
            "--proc".into(),
            "/proc".into(),
            "--tmpfs".into(),
            "/tmp".into(),
            "--setenv".into(),
            "PATH".into(),
            SANDBOX_PATH.into(),
            "--setenv".into(),
            "LC_ALL".into(),
            "C".into(),
            "--setenv".into(),
            "HOME".into(),
            "/tmp".into(),
            "--setenv".into(),
            "TMPDIR".into(),
            "/tmp".into(),
        ];
        for directory in &self.host_binds {
            args.push("--ro-bind".into());
            args.push((*directory).into());
            args.push((*directory).into());
        }
        args
    }
}

/// Exit code of a finished bwrap/shell. A signal death maps to the shell
/// convention `128 + signal` so the 137 (SIGKILL) OOM contract holds.
fn exit_code_of(status: &std::process::ExitStatus) -> i64 {
    if let Some(code) = status.code() {
        return i64::from(code);
    }
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return i64::from(128 + signal);
    }
    137
}

/// Reads the interactive-run diagnostic files straight from the host-side work
/// directory — no follow-up exec needed in this backend. Their content is
/// contestant-written, so they are only ever appended after the GNU-time
/// marker has been parsed off the run stream.
async fn read_interactive_diagnostics(work_dir: &Path) -> String {
    let mut diagnostics = String::new();
    for name in ["program.err", "interactor.err"] {
        // A contestant that never wrote diagnostics is normal.
        if let Ok(bytes) = tokio::fs::read(work_dir.join(name)).await {
            diagnostics.push_str(&String::from_utf8_lossy(&bytes));
            diagnostics.push('\n');
        }
    }
    diagnostics
}

/// Drains a sandbox output stream into a bounded log. After the cap the stream
/// keeps being drained (so a chatty contestant cannot block on a full pipe)
/// but nothing more is retained.
async fn collect_bounded_stream<R: AsyncRead + Unpin>(stream: &mut R) -> String {
    let mut logs: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let room = MAX_EXEC_LOG_BYTES.saturating_sub(logs.len());
                logs.extend_from_slice(&buffer[..read.min(room)]);
            }
        }
    }
    String::from_utf8_lossy(&logs).into_owned()
}

/// Resolves a binary name through the worker's `PATH`.
pub(crate) fn resolve_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[async_trait::async_trait]
impl crate::sandbox::SandboxJanitor for BubblewrapSandbox {
    /// Reclaims leftover job directories and leftover per-action sandbox
    /// cgroups (worker SIGKILLs can leak both). There are no containers in
    /// this backend.
    async fn sweep_orphans(
        &self,
        keep: &std::collections::HashSet<Uuid>,
    ) -> Result<crate::sandbox::OrphanSweep, SandboxError> {
        let job_dirs = crate::sandbox::gc::sweep_orphan_job_dirs(&self.cache_dir, keep).await?;
        let cgroups = match &self.cgroup {
            Some(manager) => {
                manager.sweep().await.map_err(|error| SandboxError::Api(error.to_string()))?
            }
            None => 0,
        };
        Ok(crate::sandbox::OrphanSweep { containers: 0, job_dirs, cgroups })
    }
}

/// POSIX shells express `ulimit -f` in 512-byte blocks, while the task contract
/// uses KiB. Keep the kernel file limit and the post-run byte check on the same
/// boundary.
fn output_file_blocks(output_limit_kb: i32) -> i64 {
    i64::from(output_limit_kb).saturating_mul(2).max(1)
}

#[cfg(test)]
mod tests {
    use super::{exit_code_of, output_file_blocks, resolve_in_path};

    #[test]
    fn output_blocks_convert_kib_to_512_byte_blocks() {
        assert_eq!(output_file_blocks(64), 128);
        assert_eq!(output_file_blocks(1), 2);
        assert_eq!(output_file_blocks(0), 1);
    }

    #[test]
    fn exit_codes_map_signal_deaths_to_128_plus_signal() {
        let finished = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 3"])
            .status()
            .expect("spawn probe");
        assert_eq!(exit_code_of(&finished), 3);

        let killed = std::process::Command::new("/bin/sh")
            .args(["-c", "kill -9 $$"])
            .status()
            .expect("spawn probe");
        assert_eq!(exit_code_of(&killed), 137);
    }

    #[test]
    fn path_resolution_finds_present_binaries_only() {
        assert!(resolve_in_path("sh").is_some());
        assert!(resolve_in_path("pb-definitely-missing-tool").is_none());
    }
}
