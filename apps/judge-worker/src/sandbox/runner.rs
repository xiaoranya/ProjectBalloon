use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use bollard::{
    Docker,
    exec::{StartExecOptions, StartExecResults},
    models::{ContainerCreateBody, ContainerUpdateBody, ExecConfig, HostConfig, Mount, MountType},
    query_parameters::{
        InspectContainerOptions, RemoveContainerOptionsBuilder, StartContainerOptions,
    },
};
use futures_util::StreamExt;
use project_balloon_contracts::{JudgeRunResult, JudgeTask, JudgeVerdict};
use tokio::time::{Instant, timeout};

use super::archive::{extract_cases, extract_output_cases};
use super::compare::standard_output_matches;
use super::fs::{
    create_private_dir, nonempty, read_regular_output_no_follow, remove_dir_if_present,
    remove_file_if_present, set_executable_file_permissions, set_private_file_permissions,
    truncate_log,
};
use super::language::LanguageConfig;
use super::metrics::{
    ContainerResourceUsage, collect_resource_usage, extract_gnu_time_metrics, nonzero_milliseconds,
};
use super::{
    DOCKER_API_TIMEOUT, DockerSandbox, DockerSandboxConfig, MAX_EXEC_LOG_BYTES, SandboxError,
    SandboxJudgement,
};

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

struct ContainerRun {
    exit_code: i64,
    timed_out: bool,
    oom_killed: bool,
    elapsed_ms: i32,
    cpu_time_ms: Option<i32>,
    peak_memory_kb: i32,
    logs: String,
}
