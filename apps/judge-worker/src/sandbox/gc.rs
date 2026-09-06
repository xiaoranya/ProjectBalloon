use std::{collections::HashMap, collections::HashSet, path::Path, time::Duration};

use bollard::query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder};
use tokio::sync::watch;
use tracing::{info, warn};
use uuid::Uuid;

use crate::rabbit::InFlightTasks;
use crate::sandbox::{DockerSandbox, JUDGE_CONTAINER_PREFIX, SandboxError, is_container_missing};

/// What one orphan sweep reclaimed. The container backend fills `containers`;
/// the bubblewrap backend fills `cgroups`; job directories apply to both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrphanSweep {
    pub containers: usize,
    pub job_dirs: usize,
    pub cgroups: usize,
}

/// Leftover reclamation behind [`JudgeEngine`], backend-agnostic: every
/// sandbox implementation cleans the resources it owns (container backend:
/// containers + job directories; bubblewrap backend: job directories +
/// sandbox cgroups). Runs at startup (nothing is in flight yet, so every
/// leftover belongs to a SIGKILLed/OOMed run or an expired drain) and
/// periodically afterwards for the leftovers of cancelled handlers.
#[async_trait::async_trait]
pub trait SandboxJanitor: Send + Sync {
    async fn sweep_orphans(&self, keep: &HashSet<Uuid>) -> Result<OrphanSweep, SandboxError>;
}

#[async_trait::async_trait]
impl SandboxJanitor for DockerSandbox {
    /// Force-removes every `pb-judge-*` container and job directory that does
    /// not belong to a judgement in `keep`.
    async fn sweep_orphans(&self, keep: &HashSet<Uuid>) -> Result<OrphanSweep, SandboxError> {
        let containers = self.sweep_orphan_containers(keep).await?;
        let job_dirs = sweep_orphan_job_dirs(&self.cache_dir, keep).await?;
        Ok(OrphanSweep { containers, job_dirs, cgroups: 0 })
    }
}

impl DockerSandbox {
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
}

/// Removes every job directory that does not belong to a judgement in `keep`.
/// Shared by both sandbox backends: job directories live under the worker's
/// own cache and never touch the container engine.
pub(crate) async fn sweep_orphan_job_dirs(
    cache_dir: &Path,
    keep: &HashSet<Uuid>,
) -> Result<usize, SandboxError> {
    let jobs_dir = cache_dir.join("jobs");
    let mut entries = match tokio::fs::read_dir(&jobs_dir).await {
        Ok(entries) => entries,
        // No jobs directory yet: nothing to sweep.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(SandboxError::Io(crate::sandbox::fs::with_path_context(
                error,
                "list job directory",
                &jobs_dir,
            )));
        }
    };
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        SandboxError::Io(crate::sandbox::fs::with_path_context(
            error,
            "read job directory entry",
            &jobs_dir,
        ))
    })? {
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

/// Sweeps orphans on every `interval` tick until the shutdown watch flips.
/// The first tick of a tokio interval fires immediately, so the loop starts
/// with a sweep even though `main` runs one explicitly before the consumer
/// starts (nothing may race that first sweep while deliveries arrive).
pub async fn run_orphan_sweeps<S: SandboxJanitor>(
    sandbox: S,
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

    use super::sweep_orphan_job_dirs;
    use crate::sandbox::{
        JUDGE_CONTAINER_PREFIX, judgement_container_name, judgement_id_from_container_name,
    };

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

        let removed = sweep_orphan_job_dirs(&root, &HashSet::from([in_flight]))
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
        let removed = sweep_orphan_job_dirs(&root, &HashSet::new()).await.expect("sweep");
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
