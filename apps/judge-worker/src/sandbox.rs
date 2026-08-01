use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bollard::{
    Docker,
    exec::{StartExecOptions, StartExecResults},
    models::{ContainerCreateBody, ContainerUpdateBody, ExecConfig, HostConfig, Mount, MountType},
    query_parameters::{
        InspectContainerOptions, RemoveContainerOptionsBuilder, StartContainerOptions,
        StatsOptionsBuilder,
    },
};
use futures_util::StreamExt;
use project_balloon_contracts::{JudgeRunResult, JudgeTask, JudgeVerdict};
use thiserror::Error;
use tokio::time::{Instant, timeout};

const MAX_EXEC_LOG_BYTES: usize = 64 * 1024;
const DOCKER_API_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_TESTDATA_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TESTDATA_FILES: usize = 10_000;
const MAX_TESTDATA_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox API failed: {0}")]
    Api(String),
    #[error("sandbox filesystem failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("test-data archive is invalid: {0}")]
    InvalidTestdata(String),
    #[error("language {0} is not enabled by this Worker slice")]
    UnsupportedLanguage(String),
}

#[derive(Debug)]
pub struct SandboxJudgement {
    pub verdict: JudgeVerdict,
    pub total_time_ms: i32,
    pub peak_memory_kb: i32,
    pub compile_log: Option<String>,
    pub runs: Vec<JudgeRunResult>,
}

#[derive(Clone)]
pub struct DockerSandbox {
    docker: Docker,
    cache_dir: std::path::PathBuf,
    runtime: Option<String>,
    user: String,
    c_image: String,
    cpp_image: String,
    java_image: String,
    python_image: String,
}

pub struct DockerSandboxConfig {
    pub socket: std::path::PathBuf,
    pub cache_dir: std::path::PathBuf,
    pub runtime: Option<String>,
    pub user: String,
    pub c_image: String,
    pub cpp_image: String,
    pub java_image: String,
    pub python_image: String,
}

impl DockerSandbox {
    pub fn connect(config: DockerSandboxConfig) -> Result<Self, SandboxError> {
        let socket = config
            .socket
            .to_str()
            .ok_or_else(|| SandboxError::Api("sandbox socket path is not UTF-8".to_owned()))?;
        let docker = Docker::connect_with_local(socket, 10, bollard::API_DEFAULT_VERSION)
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        Ok(Self {
            docker,
            cache_dir: config.cache_dir,
            runtime: config.runtime,
            user: config.user,
            c_image: config.c_image,
            cpp_image: config.cpp_image,
            java_image: config.java_image,
            python_image: config.python_image,
        })
    }

    pub async fn preflight(&self) -> Result<(), SandboxError> {
        self.docker.ping().await.map_err(|error| SandboxError::Api(error.to_string()))?;
        for image in [&self.c_image, &self.cpp_image, &self.java_image, &self.python_image] {
            self.docker
                .inspect_image(image)
                .await
                .map_err(|error| SandboxError::Api(format!("runtime image {image}: {error}")))?;
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
            (Ok(_), Err(error)) => Err(SandboxError::Io(error)),
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
        if task.judge_mode == project_balloon_contracts::JudgeMode::OutputOnly {
            return self.judge_output_only(task, source, archive, job_dir).await;
        }
        let language = LanguageConfig::for_task(task)?;
        let work_dir = job_dir.join("work");
        create_private_dir(&work_dir).await?;
        let source_path = work_dir.join(language.source_filename());
        tokio::fs::write(&source_path, source).await?;
        set_private_file_permissions(&source_path).await?;
        if let Some(interactor) = interactor {
            let path = work_dir.join("interactor");
            tokio::fs::write(&path, interactor).await?;
            set_executable_file_permissions(&path).await?;
        }
        let data_dir = job_dir.join("data");
        create_private_dir(&data_dir).await?;
        let case_count = extract_cases(archive.to_owned(), data_dir.clone()).await?;
        let image = match language {
            LanguageConfig::C => &self.c_image,
            LanguageConfig::Cpp => &self.cpp_image,
            LanguageConfig::Java => &self.java_image,
            LanguageConfig::Python => &self.python_image,
        };
        let run_memory_bytes = i64::from(task.memory_limit_mb) * 1024 * 1024;
        let container_id = self
            .create_judgement_container(
                &format!("pb-judge-{}", task.judgement_id),
                image,
                &work_dir,
                run_memory_bytes.max(1024 * 1024 * 1024),
            )
            .await?;
        let result = self
            .judge_in_container(
                &container_id,
                task,
                language,
                &work_dir,
                &data_dir,
                case_count,
                run_memory_bytes,
                task.judge_mode == project_balloon_contracts::JudgeMode::Interactive,
            )
            .await;
        let cleanup = match timeout(
            DOCKER_API_TIMEOUT,
            self.docker.remove_container(
                &container_id,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            ),
        )
        .await
        {
            Ok(result) => result.map_err(|error| SandboxError::Api(error.to_string())),
            Err(_) => Err(SandboxError::Api("timed out removing sandbox container".to_owned())),
        };
        match (result, cleanup) {
            (Ok(judgement), Ok(())) => Ok(judgement),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    async fn judge_output_only(
        &self,
        _task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        job_dir: &Path,
    ) -> Result<SandboxJudgement, SandboxError> {
        let data_dir = job_dir.join("data");
        create_private_dir(&data_dir).await?;
        let case_count = extract_cases(archive.to_owned(), data_dir.clone()).await?;
        let output_dir = job_dir.join("outputs");
        create_private_dir(&output_dir).await?;
        extract_output_cases(source.to_owned(), output_dir.clone()).await?;
        let mut runs = Vec::with_capacity(case_count);
        for test_index in 1..=case_count {
            let expected = tokio::fs::read(data_dir.join(format!("{test_index}.out"))).await?;
            let actual = tokio::fs::read(output_dir.join(format!("{test_index}.out"))).await.ok();
            let verdict = if actual
                .as_deref()
                .is_some_and(|actual| standard_output_matches(&expected, actual))
            {
                JudgeVerdict::Accepted
            } else {
                JudgeVerdict::WrongAnswer
            };
            runs.push(JudgeRunResult {
                test_index: i32::try_from(test_index).unwrap_or(i32::MAX),
                verdict,
                time_ms: 0,
                memory_kb: 0,
                exit_code: Some(0),
                stderr_tail: None,
            });
        }
        let verdict = if runs.iter().all(|run| run.verdict == JudgeVerdict::Accepted) {
            JudgeVerdict::Accepted
        } else {
            JudgeVerdict::WrongAnswer
        };
        Ok(SandboxJudgement {
            verdict,
            total_time_ms: 0,
            peak_memory_kb: 0,
            compile_log: None,
            runs,
        })
    }

    // Sandbox limits and paths are passed separately to keep the container policy visible.
    #[allow(clippy::too_many_arguments)]
    async fn judge_in_container(
        &self,
        container_id: &str,
        task: &JudgeTask,
        language: LanguageConfig,
        work_dir: &Path,
        data_dir: &Path,
        case_count: usize,
        run_memory_bytes: i64,
        interactive: bool,
    ) -> Result<SandboxJudgement, SandboxError> {
        let compile = self
            .run_exec(container_id, language.compile_command(), Duration::from_secs(30))
            .await?;
        self.kill_contestant_processes(container_id).await?;
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

        self.docker
            .update_container(
                container_id,
                ContainerUpdateBody {
                    memory: Some(run_memory_bytes),
                    memory_swap: Some(run_memory_bytes),
                    ..ContainerUpdateBody::default()
                },
            )
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;

        let mut runs = Vec::with_capacity(case_count);
        let mut total_time_ms = 0_i32;
        let effective_time_limit_ms =
            (f64::from(task.time_limit_ms) * task.language_multiplier).ceil();
        let effective_time_limit_ms = effective_time_limit_ms.clamp(1.0, f64::from(i32::MAX));
        let effective_time_limit_ms = effective_time_limit_ms as i32;
        let wall_limit = Duration::from_millis(
            u64::try_from(effective_time_limit_ms).unwrap_or(1).saturating_mul(3).max(1_000),
        );
        for test_index in 1..=case_count {
            let input_path = work_dir.join("current.in");
            tokio::fs::copy(data_dir.join(format!("{test_index}.in")), &input_path).await?;
            set_private_file_permissions(&input_path).await?;
            let actual_path = work_dir.join("actual.out");
            remove_file_if_present(&actual_path).await?;
            // POSIX shells express `ulimit -f` in 512-byte blocks, while the task contract uses
            // KiB. Keep the kernel file limit and the post-run byte check on the same boundary.
            let output_blocks = i64::from(task.output_limit_kb).saturating_mul(2).max(1);
            let program = language.run_command(task.memory_limit_mb);
            let shell = if interactive {
                format!(
                    "export LC_ALL=C; ulimit -f {output_blocks}; rm -f /work/to_program /work/to_interactor /work/actual.out /work/program.status /work/time.err; mkfifo /work/to_program /work/to_interactor; exec 3<>/work/to_program; exec 4<>/work/to_interactor; /work/interactor /work/current.in <&4 >&3 2>/work/interactor.err & interactor_pid=$!; /usr/bin/time --quiet --format '__PROJECT_BALLOON_GNU_TIME__ %U %S %M' 2>/work/time.err sh -c '{program} <&3 2>/work/program.err; printf \"%s\" \"$?\" >/work/program.status' | tee /work/actual.out >&4 & program_pid=$!; exec 3>&- 4>&-; wait $program_pid; program_status=$(cat /work/program.status 2>/dev/null || printf '1'); wait $interactor_pid; interactor_status=$?; cat /work/program.err /work/interactor.err /work/time.err >&2; [ $program_status -eq 0 ] || exit 10; [ $interactor_status -eq 0 ] || exit 20"
                )
            } else {
                format!(
                    "export LC_ALL=C; ulimit -f {output_blocks}; exec /usr/bin/time --quiet --format '__PROJECT_BALLOON_GNU_TIME__ %U %S %M' {program} < /work/current.in > /work/actual.out"
                )
            };
            let command = vec!["/bin/sh".to_owned(), "-c".to_owned(), shell];
            let mut run = self.run_exec(container_id, command, wall_limit).await?;
            self.kill_contestant_processes(container_id).await?;
            let (sanitized_logs, gnu_time) = extract_gnu_time_metrics(&run.logs);
            run.logs = sanitized_logs;
            if gnu_time.is_none() && !run.timed_out && !run.oom_killed && run.exit_code != 137 {
                return Err(SandboxError::Api(format!(
                    "GNU time did not produce resource metrics for a completed run (exit_code={}, stderr={:?})",
                    run.exit_code, run.logs
                )));
            }
            let charged_time_ms = gnu_time
                .map(|metrics| metrics.cpu_time_ms)
                .or(run.cpu_time_ms)
                .unwrap_or(run.elapsed_ms);
            let peak_memory_kb =
                gnu_time.map_or(run.peak_memory_kb, |metrics| metrics.peak_memory_kb);
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
            let verdict = if run.oom_killed || (run.exit_code == 137 && !run.timed_out) {
                JudgeVerdict::MemoryLimitExceeded
            } else if output_bytes > output_limit_bytes
                || (run.exit_code != 0 && output_bytes >= output_limit_bytes)
            {
                JudgeVerdict::OutputLimitExceeded
            } else if run.timed_out || charged_time_ms > effective_time_limit_ms {
                JudgeVerdict::TimeLimitExceeded
            } else if interactive && run.exit_code == 20 {
                JudgeVerdict::WrongAnswer
            } else if run.exit_code != 0 || output.is_none() {
                JudgeVerdict::RuntimeError
            } else if interactive {
                JudgeVerdict::Accepted
            } else {
                let expected = tokio::fs::read(data_dir.join(format!("{test_index}.out"))).await?;
                if standard_output_matches(&expected, output.as_deref().unwrap_or_default()) {
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

    async fn create_judgement_container(
        &self,
        name: &str,
        image: &str,
        work_dir: &Path,
        memory_bytes: i64,
    ) -> Result<String, SandboxError> {
        let mounts = vec![Mount {
            target: Some("/work".to_owned()),
            source: Some(work_dir.to_string_lossy().into_owned()),
            typ: Some(MountType::BIND),
            read_only: Some(false),
            ..Mount::default()
        }];
        let host_config = HostConfig {
            mounts: Some(mounts),
            network_mode: Some("none".to_owned()),
            readonly_rootfs: Some(true),
            privileged: Some(false),
            cap_drop: Some(vec!["ALL".to_owned()]),
            security_opt: Some(vec!["no-new-privileges:true".to_owned()]),
            pids_limit: Some(64),
            memory: Some(memory_bytes),
            memory_swap: Some(memory_bytes),
            nano_cpus: Some(1_000_000_000),
            auto_remove: Some(false),
            runtime: self.runtime.clone(),
            tmpfs: Some(HashMap::from([(
                "/tmp".to_owned(),
                "rw,nosuid,nodev,noexec,size=67108864".to_owned(),
            )])),
            ..HostConfig::default()
        };
        let body = ContainerCreateBody {
            image: Some(image.to_owned()),
            // Keep the reusable container's init process in the same fixed non-root
            // identity as contestant execs. Cleanup explicitly preserves PID 1.
            user: Some(self.user.clone()),
            cmd: Some(vec!["sleep".to_owned(), "infinity".to_owned()]),
            entrypoint: Some(vec![String::new()]),
            working_dir: Some("/work".to_owned()),
            network_disabled: Some(true),
            host_config: Some(host_config),
            ..ContainerCreateBody::default()
        };
        let container = self
            .docker
            .create_container(
                Some(
                    bollard::query_parameters::CreateContainerOptionsBuilder::default()
                        .name(name)
                        .build(),
                ),
                body,
            )
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        if let Err(error) =
            self.docker.start_container(&container.id, None::<StartContainerOptions>).await
        {
            let _cleanup = timeout(
                DOCKER_API_TIMEOUT,
                self.docker.remove_container(
                    &container.id,
                    Some(RemoveContainerOptionsBuilder::default().force(true).build()),
                ),
            )
            .await;
            return Err(SandboxError::Api(error.to_string()));
        }
        Ok(container.id)
    }

    async fn run_exec(
        &self,
        container_id: &str,
        command: Vec<String>,
        wall_limit: Duration,
    ) -> Result<ContainerRun, SandboxError> {
        let exec = self
            .docker
            .create_exec(
                container_id,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
                    user: Some(self.user.clone()),
                    working_dir: Some("/work".to_owned()),
                    ..ExecConfig::default()
                },
            )
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        let started = Instant::now();
        let started_exec = self
            .docker
            .start_exec(&exec.id, Some(StartExecOptions::default()))
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        let StartExecResults::Attached { mut output, .. } = started_exec else {
            return Err(SandboxError::Api("sandbox exec unexpectedly detached".to_owned()));
        };
        let resource_usage = Arc::new(ContainerResourceUsage::default());
        let stats_task = tokio::spawn(collect_resource_usage(
            self.docker.clone(),
            container_id.to_owned(),
            resource_usage.clone(),
        ));
        let deadline = Instant::now() + wall_limit;
        let mut logs = String::new();
        let output_result = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, output.next()).await {
                Ok(Some(Ok(chunk))) => {
                    // Docker multiplexes stdout/stderr here.  Retaining an unlimited
                    // diagnostic stream lets a contestant exhaust worker memory.
                    if logs.len() < MAX_EXEC_LOG_BYTES {
                        let chunk = chunk.to_string();
                        let remaining = MAX_EXEC_LOG_BYTES - logs.len();
                        let mut end = chunk.len().min(remaining);
                        while end > 0 && !chunk.is_char_boundary(end) {
                            end -= 1;
                        }
                        logs.push_str(&chunk[..end]);
                    }
                }
                Ok(Some(Err(error))) => break Err(SandboxError::Api(error.to_string())),
                Ok(None) => break Ok(false),
                Err(_) => {
                    let kill_result = match timeout(
                        DOCKER_API_TIMEOUT,
                        self.docker.kill_container(
                            container_id,
                            None::<bollard::query_parameters::KillContainerOptions>,
                        ),
                    )
                    .await
                    {
                        Ok(result) => result
                            .map(|()| true)
                            .map_err(|error| SandboxError::Api(error.to_string())),
                        Err(_) => {
                            Err(SandboxError::Api("timed out killing sandbox container".to_owned()))
                        }
                    };
                    break kill_result;
                }
            }
        };
        stats_task.abort();
        let _stats_result = stats_task.await;
        let timed_out = output_result?;
        let exit_code = if timed_out {
            124
        } else {
            timeout(DOCKER_API_TIMEOUT, self.docker.inspect_exec(&exec.id))
                .await
                .map_err(|_| SandboxError::Api("timed out inspecting sandbox exec".to_owned()))?
                .map_err(|error| SandboxError::Api(error.to_string()))?
                .exit_code
                .ok_or_else(|| SandboxError::Api("sandbox exec has no exit code".to_owned()))?
        };
        let oom_killed = timeout(
            DOCKER_API_TIMEOUT,
            self.docker.inspect_container(container_id, None::<InspectContainerOptions>),
        )
        .await
        .map_err(|_| SandboxError::Api("timed out inspecting sandbox container".to_owned()))?
        .map_err(|error| SandboxError::Api(error.to_string()))?
        .state
        .and_then(|state| state.oom_killed)
        .unwrap_or(false);
        Ok(ContainerRun {
            exit_code,
            timed_out,
            oom_killed,
            elapsed_ms: i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX),
            cpu_time_ms: nonzero_milliseconds(resource_usage.cpu_time_ns.load(Ordering::Relaxed)),
            peak_memory_kb: i32::try_from(
                resource_usage.peak_memory_bytes.load(Ordering::Relaxed) / 1024,
            )
            .unwrap_or(i32::MAX),
            logs,
        })
    }

    async fn kill_contestant_processes(&self, container_id: &str) -> Result<(), SandboxError> {
        timeout(DOCKER_API_TIMEOUT, self.kill_contestant_processes_inner(container_id))
            .await
            .map_err(|_| SandboxError::Api("timed out cleaning sandbox processes".to_owned()))?
    }

    async fn kill_contestant_processes_inner(
        &self,
        container_id: &str,
    ) -> Result<(), SandboxError> {
        let state = self
            .docker
            .inspect_container(container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?
            .state;
        if state
            .as_ref()
            .and_then(|state| state.status.as_ref())
            .is_none_or(|status| status.as_ref() != "running")
        {
            return Ok(());
        }
        let exec = self
            .docker
            .create_exec(
                container_id,
                ExecConfig {
                    cmd: Some(vec![
                        "/bin/sh".to_owned(),
                        "-c".to_owned(),
                        "self=$$; for status in /proc/[0-9]*/status; do pid=${status#/proc/}; pid=${pid%/status}; [ \"$pid\" = 1 ] || [ \"$pid\" = \"$self\" ] || kill -KILL \"$pid\" 2>/dev/null || true; done".to_owned(),
                    ]),
                    user: Some(self.user.clone()),
                    working_dir: Some("/work".to_owned()),
                    ..ExecConfig::default()
                },
            )
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        // PID 1 is the non-root reusable init process. The cleanup shell can kill
        // other processes owned by the same UID, while preserving both itself and
        // PID 1 so the container remains available for the next exec.
        let started = self
            .docker
            .start_exec(&exec.id, Some(StartExecOptions::default()))
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        let StartExecResults::Attached { mut output, .. } = started else {
            return Err(SandboxError::Api(
                "contestant process cleanup unexpectedly detached".to_owned(),
            ));
        };
        while let Some(chunk) = output.next().await {
            chunk.map_err(|error| SandboxError::Api(error.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn read_regular_output_no_follow(path: &Path) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt};

    let mut file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // Linux returns ELOOP for O_NOFOLLOW on a symbolic link. Treat it as invalid output.
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(Some(contents))
}

#[cfg(not(unix))]
fn read_regular_output_no_follow(path: &Path) -> Result<Option<Vec<u8>>, std::io::Error> {
    use std::{fs::OpenOptions, io::Read};

    let mut file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.file_type().is_file() {
        return Ok(None);
    }
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(Some(contents))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GnuTimeMetrics {
    cpu_time_ms: i32,
    peak_memory_kb: i32,
}

fn extract_gnu_time_metrics(logs: &str) -> (String, Option<GnuTimeMetrics>) {
    const PREFIX: &str = "__PROJECT_BALLOON_GNU_TIME__ ";
    let Some(marker_start) = logs.rfind(PREFIX) else {
        return (logs.to_owned(), None);
    };
    let fields_text = &logs[marker_start + PREFIX.len()..];
    let mut fields = fields_text.split_whitespace();
    let user_seconds = fields.next().and_then(|value| value.parse::<f64>().ok());
    let system_seconds = fields.next().and_then(|value| value.parse::<f64>().ok());
    let peak_memory_kb = fields.next().and_then(|value| value.parse::<i32>().ok());
    if fields.next().is_some() {
        return (logs.to_owned(), None);
    }
    let (Some(user_seconds), Some(system_seconds), Some(peak_memory_kb)) =
        (user_seconds, system_seconds, peak_memory_kb)
    else {
        return (logs.to_owned(), None);
    };
    let sanitized = logs[..marker_start].trim_end_matches(['\r', '\n']).to_owned();
    // GNU time 1.9 emits `%U` and `%S` with centisecond precision. Round each
    // field independently so floating-point addition cannot turn 120 + 30 ms
    // into a spurious 151 ms after applying a ceiling.
    let cpu_milliseconds = (user_seconds * 1_000.0).round() + (system_seconds * 1_000.0).round();
    if !cpu_milliseconds.is_finite()
        || cpu_milliseconds.is_sign_negative()
        || cpu_milliseconds > f64::from(i32::MAX)
        || peak_memory_kb < 0
    {
        return (logs.to_owned(), None);
    }
    (sanitized, Some(GnuTimeMetrics { cpu_time_ms: cpu_milliseconds as i32, peak_memory_kb }))
}

#[derive(Default)]
struct ContainerResourceUsage {
    peak_memory_bytes: AtomicU64,
    cpu_time_ns: AtomicU64,
}

async fn collect_resource_usage(
    docker: Docker,
    id: String,
    resource_usage: Arc<ContainerResourceUsage>,
) {
    let mut stats = docker
        .stats(&id, Some(StatsOptionsBuilder::default().stream(true).one_shot(false).build()));
    while let Some(sample) = stats.next().await {
        let Ok(sample) = sample else { return };
        if let Some(memory) = sample.memory_stats {
            let usage = memory.max_usage.or(memory.usage).unwrap_or(0);
            resource_usage.peak_memory_bytes.fetch_max(usage, Ordering::Relaxed);
        }
        if let Some(cpu_time_ns) =
            sample.cpu_stats.and_then(|stats| stats.cpu_usage).and_then(|usage| usage.total_usage)
        {
            resource_usage.cpu_time_ns.fetch_max(cpu_time_ns, Ordering::Relaxed);
        }
    }
}

fn nonzero_milliseconds(nanoseconds: u64) -> Option<i32> {
    (nanoseconds > 0).then(|| {
        let rounded_up = nanoseconds.saturating_add(999_999) / 1_000_000;
        i32::try_from(rounded_up).unwrap_or(i32::MAX)
    })
}

struct ContainerRun {
    exit_code: i64,
    timed_out: bool,
    oom_killed: bool,
    elapsed_ms: i32,
    cpu_time_ms: Option<i32>,
    peak_memory_kb: i32,
    logs: String,
}

#[derive(Clone, Copy)]
enum LanguageConfig {
    C,
    Cpp,
    Java,
    Python,
}

impl LanguageConfig {
    fn for_task(task: &JudgeTask) -> Result<Self, SandboxError> {
        match task.language.as_str() {
            "c" => Ok(Self::C),
            "cpp" => Ok(Self::Cpp),
            "java" => Ok(Self::Java),
            "python" => Ok(Self::Python),
            other => Err(SandboxError::UnsupportedLanguage(other.to_owned())),
        }
    }

    const fn source_filename(self) -> &'static str {
        match self {
            Self::C => "main.c",
            Self::Cpp => "main.cpp",
            Self::Java => "Main.java",
            Self::Python => "main.py",
        }
    }

    fn compile_command(self) -> Vec<String> {
        match self {
            Self::C | Self::Cpp => {
                let (compiler, standard) =
                    if matches!(self, Self::C) { ("gcc", "gnu11") } else { ("g++", "gnu++17") };
                vec![
                    compiler.to_owned(),
                    format!("/work/{}", self.source_filename()),
                    format!("-std={standard}"),
                    "-O2".to_owned(),
                    "-pipe".to_owned(),
                    "-o".to_owned(),
                    "/work/program".to_owned(),
                ]
            }
            Self::Java => vec![
                "javac".to_owned(),
                "-encoding".to_owned(),
                "UTF-8".to_owned(),
                "-d".to_owned(),
                "/work".to_owned(),
                "/work/Main.java".to_owned(),
            ],
            Self::Python => vec![
                "python3".to_owned(),
                "-I".to_owned(),
                "-m".to_owned(),
                "py_compile".to_owned(),
                "/work/main.py".to_owned(),
            ],
        }
    }

    fn run_command(self, memory_limit_mb: i32) -> String {
        match self {
            Self::C | Self::Cpp => "/work/program".to_owned(),
            Self::Java => {
                let heap_mb = (memory_limit_mb / 2).max(16);
                format!("java -Xms16m -Xmx{heap_mb}m -cp /work Main")
            }
            Self::Python => "python3 -I -B /work/main.py".to_owned(),
        }
    }
}

async fn extract_cases(
    archive: std::path::PathBuf,
    destination: std::path::PathBuf,
) -> Result<usize, SandboxError> {
    tokio::task::spawn_blocking(move || extract_cases_blocking(&archive, &destination))
        .await
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?
}

async fn extract_output_cases(
    archive: Vec<u8>,
    destination: std::path::PathBuf,
) -> Result<(), SandboxError> {
    tokio::task::spawn_blocking(move || extract_output_cases_blocking(&archive, &destination))
        .await
        .map_err(|error| SandboxError::InvalidTestdata(error.to_string()))?
}

fn extract_output_cases_blocking(archive: &[u8], destination: &Path) -> Result<(), SandboxError> {
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

fn standard_output_matches(expected: &[u8], actual: &[u8]) -> bool {
    normalized_lines(expected) == normalized_lines(actual)
}

fn normalized_lines(content: &[u8]) -> Vec<Vec<u8>> {
    let mut normalized = Vec::with_capacity(content.len());
    let mut index = 0;
    while index < content.len() {
        if content[index] == b'\r' {
            normalized.push(b'\n');
            if content.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
        } else {
            normalized.push(content[index]);
        }
        index += 1;
    }
    let mut lines: Vec<Vec<u8>> = normalized
        .split(|byte| *byte == b'\n')
        .map(|line| {
            let end = line
                .iter()
                .rposition(|byte| !matches!(byte, b' ' | b'\t'))
                .map_or(0, |position| position + 1);
            line[..end].to_vec()
        })
        .collect();
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

fn truncate_log(log: &str, max_bytes: usize) -> String {
    let end = log
        .char_indices()
        .take_while(|(index, character)| index + character.len_utf8() <= max_bytes)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    log[..end].to_owned()
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

async fn remove_dir_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn remove_file_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn create_private_dir(path: &Path) -> Result<(), std::io::Error> {
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700)).await?;
    Ok(())
}

async fn set_private_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).await?;
    Ok(())
}

async fn set_executable_file_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    tokio::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{
        GnuTimeMetrics, extract_gnu_time_metrics, extract_output_cases_blocking,
        standard_output_matches,
    };

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
}
