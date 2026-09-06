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
use tracing::warn;

use crate::sandbox::archive::{OutputArchiveError, extract_cases, extract_output_cases};
use crate::sandbox::compare::standard_output_matches;
use crate::sandbox::fs::{
    create_private_dir, nonempty, read_regular_output_no_follow, remove_dir_if_present,
    remove_file_if_present, set_executable_file_permissions, set_private_file_permissions,
    truncate_log, with_path_context,
};
use crate::sandbox::language::LanguageConfig;
use crate::sandbox::metrics::{
    ContainerResourceUsage, GNU_TIME_REPORT_FORMAT, collect_resource_usage,
    extract_gnu_time_metrics, nonzero_milliseconds, snapshot_container_cpu,
};
use crate::sandbox::{
    COMPILE_WALL_LIMIT, DockerSandbox, DockerSandboxConfig, MAX_EXEC_LOG_BYTES, SandboxError,
    SandboxJudgement, effective_time_limit, is_container_missing, is_container_name_conflict,
    judgement_container_name, run_wall_limit,
};

impl DockerSandbox {
    pub fn connect(config: DockerSandboxConfig) -> Result<Self, SandboxError> {
        let socket = config
            .socket
            .to_str()
            .ok_or_else(|| SandboxError::Api("sandbox socket path is not UTF-8".to_owned()))?;
        let docker = Docker::connect_with_local(
            socket,
            config.docker_connect_timeout_seconds,
            bollard::API_DEFAULT_VERSION,
        )
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
            go_image: config.go_image,
            rust_image: config.rust_image,
            docker_api_timeout: config.docker_api_timeout,
        })
    }

    pub async fn preflight(&self) -> Result<(), SandboxError> {
        self.docker.ping().await.map_err(|error| SandboxError::Api(error.to_string()))?;
        for image in [
            &self.c_image,
            &self.cpp_image,
            &self.java_image,
            &self.python_image,
            &self.go_image,
            &self.rust_image,
        ] {
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
        if task.judge_mode == project_balloon_contracts::JudgeMode::OutputOnly {
            return self.judge_output_only(task, source, archive, job_dir).await;
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
        let case_count = extract_cases(archive.to_owned(), data_dir.clone()).await?;
        let image = match language {
            LanguageConfig::C => &self.c_image,
            LanguageConfig::Cpp => &self.cpp_image,
            LanguageConfig::Java => &self.java_image,
            LanguageConfig::Python => &self.python_image,
            LanguageConfig::Go => &self.go_image,
            LanguageConfig::Rust => &self.rust_image,
        };
        let run_memory_bytes = i64::from(task.memory_limit_mb) * 1024 * 1024;
        let container_id = self
            .create_judgement_container(
                &judgement_container_name(task.judgement_id),
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
            self.docker_api_timeout,
            self.docker.remove_container(
                &container_id,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            ),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            // Already gone: a concurrent sweep or a previous removal won.
            Ok(Err(error)) if is_container_missing(&error) => Ok(()),
            Ok(Err(error)) => Err(SandboxError::Api(error.to_string())),
            Err(_) => Err(SandboxError::Api("timed out removing sandbox container".to_owned())),
        };
        match (result, cleanup) {
            (Ok(judgement), Ok(())) => Ok(judgement),
            (Err(error), _) => Err(error),
            // A finished judgement is never re-run because its container could
            // not be removed: the leftover is reclaimed by the orphan sweeper.
            (Ok(judgement), Err(error)) => {
                warn!(
                    judgement_id = %task.judgement_id,
                    container_id = %container_id,
                    error = %error,
                    "sandbox container cleanup failed after a completed judgement; keeping the result"
                );
                Ok(judgement)
            }
        }
    }

    async fn judge_output_only(
        &self,
        task: &JudgeTask,
        source: &[u8],
        archive: &Path,
        job_dir: &Path,
    ) -> Result<SandboxJudgement, SandboxError> {
        run_output_only(task, source, archive, job_dir).await
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
            .run_exec(
                container_id,
                language.compile_command(),
                &language.compile_env(),
                COMPILE_WALL_LIMIT,
            )
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

        if let Err(update_error) = self
            .docker
            .update_container(
                container_id,
                ContainerUpdateBody {
                    memory: Some(run_memory_bytes),
                    memory_swap: Some(run_memory_bytes),
                    ..ContainerUpdateBody::default()
                },
            )
            .await
        {
            let fallback = self
                .docker
                .update_container(
                    container_id,
                    ContainerUpdateBody {
                        memory: Some(run_memory_bytes),
                        ..ContainerUpdateBody::default()
                    },
                )
                .await;
            memory_update_outcome(update_error, fallback)?;
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
            let shell = if interactive {
                interactive_shell("/usr/bin/time", &program, output_blocks)
            } else {
                standard_shell("/usr/bin/time", &program, output_blocks)
            };
            let command = vec!["/bin/sh".to_owned(), "-c".to_owned(), shell];
            let mut run = self.run_exec(container_id, command, &[], wall_limit).await?;
            self.kill_contestant_processes(container_id).await?;
            // Contestant-writable diagnostic files are fetched in a separate
            // exec and appended only AFTER the GNU-time marker has been parsed:
            // the parser trusts the last marker, so forged bytes must never be
            // able to follow the real report in the stream it parses.
            let diagnostics = if interactive {
                self.fetch_interactor_diagnostics(container_id).await
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
            } else if interactive && run.exit_code == 20 {
                JudgeVerdict::WrongAnswer
            } else if run.exit_code != 0 || output.is_none() {
                JudgeVerdict::RuntimeError
            } else if interactive {
                JudgeVerdict::Accepted
            } else {
                let expected_path = data_dir.join(format!("{test_index}.out"));
                let expected = tokio::fs::read(&expected_path).await.map_err(|error| {
                    with_path_context(error, "read expected output", &expected_path)
                })?;
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

    /// Force-removes a sandbox container addressed by name, tolerating an
    /// already-vanished target.
    pub(super) async fn remove_container_by_name(&self, name: &str) -> Result<(), SandboxError> {
        match timeout(
            self.docker_api_timeout,
            self.docker.remove_container(
                name,
                Some(RemoveContainerOptionsBuilder::default().force(true).build()),
            ),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) if is_container_missing(&error) => Ok(()),
            Ok(Err(error)) => Err(SandboxError::Api(error.to_string())),
            Err(_) => Err(SandboxError::Api("timed out removing sandbox container".to_owned())),
        }
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
        let options =
            bollard::query_parameters::CreateContainerOptionsBuilder::default().name(name).build();
        let container = match self
            .docker
            .create_container(Some(options.clone()), body.clone())
            .await
        {
            Ok(container) => container,
            // A previous run of this judgement (SIGKILL, OOM, drain expiry)
            // can leave its container behind. Remove the stale one once and
            // retry instead of failing the redelivered task.
            Err(error) if is_container_name_conflict(&error) => {
                warn!(name = %name, error = %error, "sandbox container name already exists; removing the stale container and retrying");
                self.remove_container_by_name(name).await?;
                self.docker
                    .create_container(Some(options), body)
                    .await
                    .map_err(|error| SandboxError::Api(error.to_string()))?
            }
            Err(error) => return Err(SandboxError::Api(error.to_string())),
        };
        if let Err(error) =
            self.docker.start_container(&container.id, None::<StartContainerOptions>).await
        {
            let _cleanup = timeout(
                self.docker_api_timeout,
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
        env: &[String],
        wall_limit: Duration,
    ) -> Result<ContainerRun, SandboxError> {
        // Baseline for the CPU-time fallback: docker stats totals are
        // cumulative for the whole container, so the exec's share is the
        // difference between two snapshots taken around the exec.
        let base_cpu_ns = snapshot_container_cpu(&self.docker, container_id).await;
        let exec = self
            .docker
            .create_exec(
                container_id,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
                    env: if env.is_empty() { None } else { Some(env.to_vec()) },
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
                        self.docker_api_timeout,
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
        let streamed_cpu_ns = resource_usage.cpu_time_ns.load(Ordering::Relaxed);
        let docker_cpu_ns =
            match (base_cpu_ns, snapshot_container_cpu(&self.docker, container_id).await) {
                // Both snapshots landed: the exec's own CPU time.
                (Some(base), Some(total)) => total.saturating_sub(base),
                // The trailing snapshot failed (container killed or exited during
                // a timeout/OOM) — subtract the baseline from the last streamed
                // cumulative sample instead.
                (Some(base), None) => streamed_cpu_ns.saturating_sub(base),
                // No baseline at all: fall back to the raw cumulative value, which
                // errs on the high side but never fakes a timeout.
                (None, _) => streamed_cpu_ns,
            };
        let exit_code = if timed_out {
            124
        } else {
            timeout(self.docker_api_timeout, self.docker.inspect_exec(&exec.id))
                .await
                .map_err(|_| SandboxError::Api("timed out inspecting sandbox exec".to_owned()))?
                .map_err(|error| SandboxError::Api(error.to_string()))?
                .exit_code
                .ok_or_else(|| SandboxError::Api("sandbox exec has no exit code".to_owned()))?
        };
        let oom_killed = timeout(
            self.docker_api_timeout,
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
            cpu_time_ms: nonzero_milliseconds(docker_cpu_ns),
            peak_memory_kb: i32::try_from(
                resource_usage.peak_memory_bytes.load(Ordering::Relaxed) / 1024,
            )
            .unwrap_or(i32::MAX),
            logs,
        })
    }

    async fn kill_contestant_processes(&self, container_id: &str) -> Result<(), SandboxError> {
        timeout(self.docker_api_timeout, self.kill_contestant_processes_inner(container_id))
            .await
            .map_err(|_| SandboxError::Api("timed out cleaning sandbox processes".to_owned()))?
    }

    /// Reads the interactive-run diagnostic files (contestant and interactor
    /// stderr) in a follow-up exec. Their content is contestant-writable, so
    /// they must never be concatenated into the exec stream the GNU-time
    /// marker is parsed from, and losing them must not fail a completed run.
    async fn fetch_interactor_diagnostics(&self, container_id: &str) -> String {
        let command = vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "cat /work/program.err /work/interactor.err 2>/dev/null".to_owned(),
        ];
        match self.run_exec(container_id, command, &[], COMPILE_WALL_LIMIT).await {
            Ok(run) => truncate_log(&run.logs, MAX_EXEC_LOG_BYTES),
            Err(error) => {
                warn!(
                    container_id = %container_id,
                    error = %error,
                    "failed to fetch interactive diagnostic logs; continuing without them"
                );
                String::new()
            }
        }
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

/// Output-only judging never touches Docker: it extracts the trusted testdata
/// archive and the contestant output archive, then compares bytes. The
/// contestant archive is authenticated contestant input, so its validation
/// failures are the submission's fault and surface as WrongAnswer — only the
/// problem-testdata path keeps the InvalidTestdata (infrastructure)
/// classification.
pub(super) async fn run_output_only(
    task: &JudgeTask,
    source: &[u8],
    archive: &Path,
    job_dir: &Path,
) -> Result<SandboxJudgement, SandboxError> {
    let data_dir = job_dir.join("data");
    create_private_dir(&data_dir).await?;
    let case_count = extract_cases(archive.to_owned(), data_dir.clone()).await?;
    let output_dir = job_dir.join("outputs");
    create_private_dir(&output_dir).await?;
    if let Err(error) = extract_output_cases(source.to_owned(), output_dir.clone()).await {
        return match error {
            // Worker-side I/O (cache directory, cancelled extraction task) is
            // not the submission's fault and stays an infrastructure failure.
            OutputArchiveError::Io(io_error) => Err(SandboxError::Io(io_error)),
            validation_error => {
                let reason = validation_error.to_string();
                warn!(
                    judgement_id = %task.judgement_id,
                    reason = %reason,
                    "output-only archive rejected as a contestant fault"
                );
                Ok(invalid_output_submission(&reason))
            }
        };
    }
    let mut runs = Vec::with_capacity(case_count);
    for test_index in 1..=case_count {
        let expected_path = data_dir.join(format!("{test_index}.out"));
        let expected = tokio::fs::read(&expected_path)
            .await
            .map_err(|error| with_path_context(error, "read expected output", &expected_path))?;
        let actual = tokio::fs::read(output_dir.join(format!("{test_index}.out"))).await.ok();
        let verdict =
            if actual.as_deref().is_some_and(|actual| standard_output_matches(&expected, actual)) {
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
    Ok(SandboxJudgement { verdict, total_time_ms: 0, peak_memory_kb: 0, compile_log: None, runs })
}

/// Builds the contestant-facing verdict for an unusable output-only archive.
/// A single synthetic WrongAnswer run carries the reason because the result
/// contract only permits empty run lists for compilation/infrastructure
/// verdicts, and this must never be reported as a SystemError.
fn invalid_output_submission(reason: &str) -> SandboxJudgement {
    SandboxJudgement {
        verdict: JudgeVerdict::WrongAnswer,
        total_time_ms: 0,
        peak_memory_kb: 0,
        compile_log: None,
        runs: vec![JudgeRunResult {
            test_index: 1,
            verdict: JudgeVerdict::WrongAnswer,
            time_ms: 0,
            memory_kb: 0,
            exit_code: Some(0),
            stderr_tail: nonempty(truncate_log(reason, 16 * 1024)),
        }],
    }
}

/// Resolves a failed memory+swap container update. Hosts without swap
/// accounting reject the swap limit while still accepting the plain memory
/// limit, so the failure is only fatal when the memory-only update fails too
/// — which is how a genuine sandbox API failure is distinguished.
fn memory_update_outcome(
    combined_error: bollard::errors::Error,
    fallback: Result<(), bollard::errors::Error>,
) -> Result<(), SandboxError> {
    match fallback {
        Ok(()) => {
            warn!(
                container_update_error = %combined_error,
                "sandbox rejected the memory+swap update (host without swap accounting?); the memory limit alone was applied"
            );
            Ok(())
        }
        Err(fallback_error) => Err(SandboxError::Api(format!(
            "sandbox container memory update failed (with swap limit: {combined_error}; without: {fallback_error})"
        ))),
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

/// POSIX shells express `ulimit -f` in 512-byte blocks, while the task contract
/// uses KiB. Keep the kernel file limit and the post-run byte check on the same
/// boundary.
fn output_file_blocks(output_limit_kb: i32) -> i64 {
    i64::from(output_limit_kb).saturating_mul(2).max(1)
}

/// Shell for a standard (non-interactive) run. The GNU-time report is the
/// terminal write on the exec stderr stream: the parser trusts the LAST
/// marker, and the contestant's own stderr can only precede the report.
/// `time_tool` is the GNU-time path *inside* the sandbox (`/usr/bin/time` for
/// the container backend, the read-only `/work/.pb-time` bind for the
/// bubblewrap backend).
pub(super) fn standard_shell(time_tool: &str, program: &str, output_blocks: i64) -> String {
    format!(
        "export LC_ALL=C; ulimit -f {output_blocks}; exec {time_tool} --quiet \
         --format '{GNU_TIME_REPORT_FORMAT}' {program} < /work/current.in > /work/actual.out"
    )
}

/// Shell for an interactive run. The GNU-time report goes straight to the exec
/// stderr stream — a channel the contestant process holds no descriptor for.
/// It must not be redirected into a /work file: every file under /work is
/// writable by the contestant UID, and the old script concatenated
/// `/work/time.err` (plus the contestant's own `/work/program.err`) into the
/// stream AFTER the report, letting a forged last marker reset the charged
/// CPU time and peak memory. The diagnostic files are read back separately by
/// [`DockerSandbox::fetch_interactor_diagnostics`].
pub(super) fn interactive_shell(time_tool: &str, program: &str, output_blocks: i64) -> String {
    format!(
        "export LC_ALL=C; ulimit -f {output_blocks}; rm -f /work/to_program /work/to_interactor \
         /work/actual.out /work/program.status /work/program.err /work/interactor.err; \
         mkfifo /work/to_program /work/to_interactor; exec 3<>/work/to_program; \
         exec 4<>/work/to_interactor; /work/interactor /work/current.in <&4 >&3 \
         2>/work/interactor.err & interactor_pid=$!; {time_tool} --quiet \
         --format '{GNU_TIME_REPORT_FORMAT}' sh -c '{program} <&3 2>/work/program.err; \
         printf \"%s\" \"$?\" >/work/program.status' | tee /work/actual.out >&4 & \
         program_pid=$!; exec 3>&- 4>&-; wait $program_pid; program_status=$(cat \
         /work/program.status 2>/dev/null || printf '1'); wait $interactor_pid; \
         interactor_status=$?; [ $program_status -eq 0 ] || exit 10; \
         [ $interactor_status -eq 0 ] || exit 20"
    )
}

/// Decides the resource-driven verdicts (memory, output, time). These strictly
/// precede the interactive protocol and the byte comparison; `None` means the
/// run finished inside its limits and the output must be judged.
pub(super) fn resource_verdict(
    oom_killed: bool,
    exit_code: i64,
    timed_out: bool,
    output_bytes: u64,
    output_limit_bytes: u64,
    charged_time_ms: i32,
    effective_time_limit_ms: i32,
) -> Option<JudgeVerdict> {
    if oom_killed || (exit_code == 137 && !timed_out) {
        Some(JudgeVerdict::MemoryLimitExceeded)
    } else if output_bytes > output_limit_bytes
        || (exit_code != 0 && output_bytes >= output_limit_bytes)
    {
        Some(JudgeVerdict::OutputLimitExceeded)
    } else if timed_out || charged_time_ms > effective_time_limit_ms {
        Some(JudgeVerdict::TimeLimitExceeded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use project_balloon_contracts::JudgeVerdict;

    use super::{
        GNU_TIME_REPORT_FORMAT, effective_time_limit, interactive_shell, output_file_blocks,
        resource_verdict, standard_shell,
    };

    #[test]
    fn resource_verdicts_precede_output_comparison() {
        // OOM kills and exit code 137 mean memory exhaustion.
        assert_eq!(
            resource_verdict(true, 0, false, 0, 1_024, 1, 1_000),
            Some(JudgeVerdict::MemoryLimitExceeded)
        );
        assert_eq!(
            resource_verdict(false, 137, false, 0, 1_024, 1, 1_000),
            Some(JudgeVerdict::MemoryLimitExceeded)
        );
        // Exit 137 with a wall-clock timeout is a time limit, not OOM.
        assert_eq!(
            resource_verdict(false, 137, true, 0, 1_024, 1, 1_000),
            Some(JudgeVerdict::TimeLimitExceeded)
        );
        // Output above the limit is OLE; a non-zero exit at the boundary is too.
        assert_eq!(
            resource_verdict(false, 0, false, 1_025, 1_024, 1, 1_000),
            Some(JudgeVerdict::OutputLimitExceeded)
        );
        assert_eq!(
            resource_verdict(false, 1, false, 1_024, 1_024, 1, 1_000),
            Some(JudgeVerdict::OutputLimitExceeded)
        );
        assert_eq!(resource_verdict(false, 0, false, 1_024, 1_024, 1, 1_000), None);
        // Wall-clock timeouts and charged CPU beyond the effective limit are TLE.
        assert_eq!(
            resource_verdict(false, 0, true, 0, 1_024, 1, 1_000),
            Some(JudgeVerdict::TimeLimitExceeded)
        );
        assert_eq!(
            resource_verdict(false, 0, false, 0, 1_024, 1_001, 1_000),
            Some(JudgeVerdict::TimeLimitExceeded)
        );
        assert_eq!(resource_verdict(false, 0, false, 0, 1_024, 1_000, 1_000), None);
    }

    #[test]
    fn effective_time_limits_apply_multiplier_and_clamp() {
        assert_eq!(effective_time_limit(1_000, 1.0), 1_000);
        assert_eq!(effective_time_limit(1_000, 2.0), 2_000);
        assert_eq!(effective_time_limit(1_000, 0.001), 1);
        assert_eq!(effective_time_limit(1, 0.0), 1);
    }

    #[test]
    fn output_blocks_convert_kib_to_512_byte_blocks() {
        assert_eq!(output_file_blocks(64), 128);
        assert_eq!(output_file_blocks(1), 2);
        assert_eq!(output_file_blocks(0), 1);
    }

    #[test]
    fn interactive_shell_keeps_the_time_report_off_contestant_writable_paths() {
        let shell = interactive_shell("/usr/bin/time", "/work/program", 128);
        // The report is parsed as the last marker on the exec stderr stream:
        // it must not be cached in a /work file (same-UID writable), and no
        // contestant-writable file may be concatenated into that stream after
        // it — those diagnostics are read back by a separate exec instead.
        assert!(!shell.contains("/work/time.err"));
        assert!(!shell.contains("cat /work/program.err"));
        assert!(!shell.contains("cat /work/interactor.err"));
        assert!(shell.contains(&format!("--format '{GNU_TIME_REPORT_FORMAT}'")));
        // Per-case diagnostics still land in their files for the follow-up
        // exec, and stale files from an earlier case are cleared first.
        assert!(shell.contains("2>/work/program.err"));
        assert!(shell.contains("2>/work/interactor.err"));
        assert!(shell.contains(
            "rm -f /work/to_program /work/to_interactor /work/actual.out /work/program.status \
             /work/program.err /work/interactor.err"
        ));
        // The status plumbing the host parses stays intact.
        assert!(shell.contains("exit 10"));
        assert!(shell.contains("exit 20"));
    }

    #[test]
    fn standard_shell_ends_with_the_report_terminal_on_stderr() {
        let shell = standard_shell("/usr/bin/time", "/work/program", 64);
        assert!(shell.contains(&format!("--format '{GNU_TIME_REPORT_FORMAT}'")));
        assert!(shell.ends_with("> /work/actual.out"));
        // Nothing may be appended after the timed command: the report must be
        // the last write on the exec stderr stream.
        assert!(!shell.contains("/work/program.err"));
    }
}

#[cfg(test)]
mod memory_and_container_tests {
    use project_balloon_contracts::JudgeVerdict;

    use super::{invalid_output_submission, memory_update_outcome};
    use crate::sandbox::{SandboxError, is_container_missing, is_container_name_conflict};

    fn docker_error(status_code: u16, message: &str) -> bollard::errors::Error {
        bollard::errors::Error::DockerResponseServerError {
            status_code,
            message: message.to_owned(),
        }
    }

    #[test]
    fn memory_swap_rejection_is_recovered_by_the_memory_only_update() {
        let kernel_without_swap_accounting =
            docker_error(500, "Your kernel does not support swap limit capabilities");
        // The memory-only fallback succeeds: the failure is not fatal.
        assert!(memory_update_outcome(kernel_without_swap_accounting, Ok(())).is_ok());

        // A genuine API failure fails the fallback too and must surface.
        let combined = docker_error(500, "driver failed programming external connectivity");
        let fallback = Err(docker_error(500, "OCI runtime update failed"));
        let error = memory_update_outcome(combined, fallback).expect_err("genuine failure");
        assert!(matches!(error, SandboxError::Api(_)));
    }

    #[test]
    fn container_conflict_and_missing_statuses_are_recognized() {
        assert!(is_container_name_conflict(&docker_error(
            409,
            "Conflict. The container name is already in use"
        )));
        assert!(!is_container_name_conflict(&docker_error(500, "internal error")));
        assert!(is_container_missing(&docker_error(404, "No such container")));
        assert!(!is_container_missing(&docker_error(409, "Conflict")));
    }

    #[test]
    fn invalid_output_submission_is_a_contract_valid_wrong_answer() {
        let judgement = invalid_output_submission("output archive is invalid: broken");
        assert_eq!(judgement.verdict, JudgeVerdict::WrongAnswer);
        assert_eq!(judgement.runs.len(), 1);
        assert_eq!(judgement.runs[0].verdict, JudgeVerdict::WrongAnswer);
        assert_eq!(judgement.runs[0].test_index, 1);
        assert_eq!(
            judgement.runs[0].stderr_tail.as_deref(),
            Some("output archive is invalid: broken")
        );

        // A bare WrongAnswer with no runs would be contract-invalid; the
        // synthetic run exists precisely to keep the result valid.
        let mut result = project_balloon_contracts::JudgeResult {
            schema_version: project_balloon_contracts::JUDGE_RESULT_SCHEMA_VERSION,
            message_id: uuid::Uuid::new_v4(),
            judgement_id: uuid::Uuid::new_v4(),
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
        result.message_id = result.judgement_id;
        result.validate().expect("contestant outcome must be contract-valid");
    }
}
