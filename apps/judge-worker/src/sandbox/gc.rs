use std::{collections::HashMap, collections::HashSet, time::Duration};

use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder};
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use crate::rabbit::InFlightTasks;
use crate::sandbox::{DockerSandbox, JUDGE_CONTAINER_PREFIX, SandboxError, is_container_missing};

/// What one orphan sweep reclaimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrphanSweep {
    pub containers: usize,
    pub job_dirs: usize,
}

impl DockerSandbox {
    /// Force-removes every `pb-judge-*` container and job directory that does
    /// not belong to a judgement in `keep`. Runs at startup (nothing is in
    /// flight yet, so every leftover belongs to a SIGKILLed/OOMed run or an
    /// expired drain) and periodically afterwards for the leftovers of
    /// cancelled handlers.
    pub async fn sweep_orphans(&self, keep: &HashSet<Uuid>) -> Result<OrphanSweep, SandboxError> {
        let containers = self.sweep_orphan_containers(keep).await?;
        let job_dirs = self.sweep_orphan_job_dirs(keep).await?;
        Ok(OrphanSweep { containers, job_dirs })
    }

    async fn sweep_orphan_containers(&self, keep: &HashSet<Uuid>) -> Result<usize, SandboxError> {
        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&HashMap::from([("name".to_owned(), vec![JUDGE_CONTAINER_PREFIX])]))
            .build();
        let containers = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(|error| SandboxError::Api(error.to_string()))?;
        let mut removed = 0;
        for container in containers {
            let Some(id) = container.id.as_deref() else { continue };
            let Some(judgement_id) = container
                .names
                .as_deref()
                .unwrap_or_default()
                .iter()
                .find_map(|name| crate::sandbox::judgement_id_from_container_name(name))
            else {
                continue;
            };
            if keep.contains(&judgement_id) {
                continue;
            }
            match self
                .docker
                .remove_container(
                    id,
                    Some(RemoveContainerOptionsBuilder::default().force(true).build()),
                )
                .await
            {
                Ok(()) => removed += 1,
                // Already gone: the task's own cleanup or a previous sweep won.
                Err(error) if is_container_missing(&error) => {}
                Err(error) => {
                    return Err(SandboxError::Api(format!(
                        "removing orphan sandbox container {id}: {error}"
                    )));
                }
            }
        }
        Ok(removed)
    }

    async fn sweep_orphan_job_dirs(&self, keep: &HashSet<Uuid>) -> Result<usize, SandboxError> {
        let jobs_dir = self.cache_dir.join("jobs");
        let mut entries = match tokio::fs::read_dir(&jobs_dir).await {
            Ok(entries) => entries,
            // No jobs directory yet: nothing to sweep.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(SandboxError::Io(error)),
        };
        let mut removed = 0;
        while let Some(entry) = entries.next_entry().await.map_err(SandboxError::Io)? {
            // Only ever touch directories named after a judgement; unknown
            // cache entries are left alone.
            let Ok(judgement_id) = entry.file_name().to_string_lossy().parse::<Uuid>() else {
                continue;
            };
            if keep.contains(&judgement_id) {
                continue;
            }
            match tokio::fs::remove_dir_all(entry.path()).await {
                Ok(()) => removed += 1,
                Err(error) => warn!(
                    path = %entry.path().display(),
                    error = %error,
                    "orphan sweep could not remove a job directory"
                ),
            }
        }
        Ok(removed)
    }
}

/// Sweeps orphans on every `interval` tick until the shutdown watch flips.
/// The first tick of a tokio interval fires immediately, so the loop starts
/// with a sweep even though `main` runs one explicitly before the consumer
/// starts (nothing may race that first sweep while deliveries arrive).
pub async fn run_orphan_sweeps(
    sandbox: DockerSandbox,
    in_flight: InFlightTasks,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) {
    info!(?interval, "sandbox orphan sweeper started");
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let keep = in_flight.judgement_ids();
                match sandbox.sweep_orphans(&keep).await {
                    Ok(sweep) if sweep.containers + sweep.job_dirs > 0 => {
                        info!(
                            containers = sweep.containers,
                            job_dirs = sweep.job_dirs,
                            "orphan sweep reclaimed sandbox resources"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!(%error, "orphan sweep failed; it retries on the next tick");
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    info!("sandbox orphan sweeper stopped");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use uuid::Uuid;

    use crate::sandbox::{
        DockerSandbox, DockerSandboxConfig, JUDGE_CONTAINER_PREFIX, judgement_container_name,
        judgement_id_from_container_name,
    };

    fn sandbox_with_cache(cache_dir: std::path::PathBuf) -> DockerSandbox {
        DockerSandbox::connect(DockerSandboxConfig {
            socket: "/var/run/docker.sock".into(),
            cache_dir,
            runtime: None,
            user: "1000:1000".to_owned(),
            c_image: "judge-runtime-c:12.2.0".to_owned(),
            cpp_image: "judge-runtime-cpp:12.2.0".to_owned(),
            java_image: "judge-runtime-java:21".to_owned(),
            python_image: "judge-runtime-python:3.12.13".to_owned(),
        })
        .expect("connect sandbox client")
    }

    #[tokio::test]
    async fn job_dir_sweep_removes_stale_dirs_and_keeps_in_flight_ones() {
        let root = std::env::temp_dir().join(format!("pb-gc-{}", Uuid::new_v4()));
        let jobs = root.join("jobs");
        let in_flight = Uuid::new_v4();
        for id in [in_flight, Uuid::new_v4(), Uuid::new_v4()] {
            let dir = jobs.join(id.to_string());
            tokio::fs::create_dir_all(&dir).await.expect("job dir");
            tokio::fs::write(dir.join("marker"), b"x").await.expect("marker");
        }
        tokio::fs::create_dir_all(jobs.join("not-a-judgement")).await.expect("unknown entry");

        let sandbox = sandbox_with_cache(root.clone());
        let removed = sandbox
            .sweep_orphan_job_dirs(&HashSet::from([in_flight]))
            .await
            .expect("sweep job dirs");

        assert_eq!(removed, 2);
        assert!(jobs.join(in_flight.to_string()).exists(), "in-flight dir must be kept");
        assert!(
            jobs.join("not-a-judgement").exists(),
            "unknown cache entries must never be removed"
        );
        tokio::fs::remove_dir_all(root).await.expect("cleanup");
    }

    #[tokio::test]
    async fn job_dir_sweep_without_jobs_directory_is_empty() {
        let root = std::env::temp_dir().join(format!("pb-gc-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.expect("root");
        let sandbox = sandbox_with_cache(root.clone());
        let removed = sandbox.sweep_orphan_job_dirs(&HashSet::new()).await.expect("sweep");
        assert_eq!(removed, 0);
        tokio::fs::remove_dir_all(root).await.expect("cleanup");
    }

    #[test]
    fn container_names_round_trip_to_judgement_ids() {
        let id = Uuid::new_v4();
        let name = judgement_container_name(id);
        assert!(name.starts_with(JUDGE_CONTAINER_PREFIX));
        assert_eq!(judgement_id_from_container_name(&format!("/{name}")), Some(id));
        assert_eq!(judgement_id_from_container_name("/other-container"), None);
    }
}
