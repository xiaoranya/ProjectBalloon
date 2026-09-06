//! cgroup v2 management for the bubblewrap (non-container) sandbox backend.
//!
//! The worker creates one throwaway cgroup per sandbox action (compile, or a
//! single test-case run) under a delegated base directory. Delegation means:
//! the base directory is writable by the worker's uid, no other processes live
//! inside it, and the worker may enable the `memory`, `pids`, and `cpu`
//! controllers on its `cgroup.subtree_control`. When `bwrap` (the
//! pid-namespace init) exits, its whole process tree is gone, so the cgroup is
//! empty again and can be removed.
//!
//! The controller limits mirror the container backend's HostConfig:
//! `memory.max`/`memory.swap.max` replace `memory`/`memory_swap`,
//! `pids.max` replaces `pids_limit`, and `cpu.max` replaces `nano_cpus`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tracing::warn;
use uuid::Uuid;

use crate::sandbox::fs::with_path_context;

/// Controllers every sandbox cgroup needs enabled on the delegated base.
pub(crate) const REQUIRED_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

/// Per-sandbox process cap, mirroring the container backend's `pids_limit`.
pub(crate) const PIDS_LIMIT: u64 = 64;

/// `cpu.max` granting one full core, mirroring `nano_cpus = 1_000_000_000`.
pub(crate) const CPU_MAX: &str = "100000 100000";

/// Prefix shared by every per-action sandbox cgroup directory, so the orphan
/// sweep can reclaim leftovers of SIGKILLed worker runs.
pub(crate) const CGROUP_PREFIX: &str = "pb-judge-";

/// Parses the controllers enabled on a `cgroup.controllers` /
/// `cgroup.subtree_control` file into a set.
pub(crate) fn parse_controller_list(contents: &str) -> BTreeSet<String> {
    contents.split_whitespace().map(str::to_owned).collect()
}

/// Builds the `cgroup.subtree_control` payload that enables the required
/// controllers without touching the ones already enabled. Re-enabling an
/// active controller fails with `EINVAL`, so only missing entries are written.
pub(crate) fn subtree_control_payload(enabled: &BTreeSet<String>) -> Option<String> {
    let missing: Vec<String> = REQUIRED_CONTROLLERS
        .iter()
        .filter(|required| !enabled.contains(**required))
        .map(|required| format!("+{required}"))
        .collect();
    (!missing.is_empty()).then(|| missing.join(" "))
}

/// Recognizes a per-action sandbox cgroup directory name:
/// `<judgement>-compile`, `<judgement>-run`, or the `probe-<judgement>` probe.
/// Unknown entries under the base are never touched.
pub(crate) fn is_sandbox_cgroup_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(CGROUP_PREFIX) else { return false };
    for suffix in ["-compile", "-run"] {
        if let Some(id) = rest.strip_suffix(suffix) {
            return Uuid::parse_str(id).is_ok();
        }
    }
    if let Some(id) = rest.strip_prefix("probe-") {
        return Uuid::parse_str(id).is_ok();
    }
    false
}

/// Extracts the `oom_kill` counter from a `memory.events` file. Returns `None`
/// when the file does not carry the key (it cannot on a healthy v2 hierarchy).
pub(crate) fn parse_oom_kill(contents: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        match (fields.next(), fields.next()) {
            (Some("oom_kill"), Some(value)) => value.parse().ok(),
            _ => None,
        }
    })
}

/// Async `write` helper that keeps the failing path in the error message.
async fn write_cgroup_file(path: &Path, contents: &str) -> Result<(), std::io::Error> {
    tokio::fs::write(path, contents)
        .await
        .map_err(|error| with_path_context(error, "write cgroup file", path))
}

/// The delegated base directory every sandbox cgroup is created under.
#[derive(Debug, Clone)]
pub(crate) struct CgroupManager {
    base: PathBuf,
}

impl CgroupManager {
    pub(crate) fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// Verifies the delegated base is usable end-to-end: the required
    /// controllers are available, `cgroup.subtree_control` accepts them, and a
    /// probe child cgroup can be created, limited, and removed.
    pub(crate) async fn probe(&self) -> Result<(), String> {
        let controllers_path = self.base.join("cgroup.controllers");
        let controllers = tokio::fs::read_to_string(&controllers_path).await.map_err(|error| {
            format!(
                "cannot read {} (is this a cgroup v2 mount, and is the directory delegated to the worker uid?): {error}",
                controllers_path.display()
            )
        })?;
        let enabled = parse_controller_list(&controllers);
        for required in REQUIRED_CONTROLLERS {
            if !enabled.contains(required) {
                return Err(format!(
                    "controller {required} is unavailable at {} (cgroup.controllers: {controllers:?})",
                    self.base.display()
                ));
            }
        }

        if let Some(payload) = subtree_control_payload(&enabled) {
            let subtree = self.base.join("cgroup.subtree_control");
            write_cgroup_file(&subtree, &payload).await.map_err(|error| {
                format!(
                    "cannot enable controllers ({payload}) on {}: {error}. The base group \
                         must have no other processes in it (cgroup v2 no-internal-process \
                         rule); point JUDGE_CGROUP_BASE at an empty delegated group.",
                    subtree.display()
                )
            })?;
        }

        let probe_name = format!("{CGROUP_PREFIX}probe-{}", Uuid::new_v4());
        let guard = self.create(&probe_name, 64 * 1024 * 1024).await.map_err(|error| {
            format!("cannot create a probe cgroup under {}: {error}", self.base.display())
        })?;
        guard.release().await;
        Ok(())
    }

    /// Creates a fresh cgroup with the sandbox limits applied. The caller must
    /// keep the [`CgroupGuard`] alive for the action and let it release.
    pub(crate) async fn create(
        &self,
        label: &str,
        memory_bytes: i64,
    ) -> Result<CgroupGuard, std::io::Error> {
        let path = self.base.join(format!("{CGROUP_PREFIX}{label}"));
        tokio::fs::create_dir(&path)
            .await
            .map_err(|error| with_path_context(error, "create sandbox cgroup", &path))?;
        let guard = CgroupGuard { path: path.clone() };
        // A failure past this point must not leak the half-configured cgroup.
        if let Err(error) = guard.apply_limits(memory_bytes) {
            guard.release().await;
            return Err(error);
        }
        Ok(guard)
    }

    /// Removes every leftover sandbox cgroup under the base (worker restarts
    /// can leak them when a SIGKILL lands between create and release).
    pub(crate) async fn sweep(&self) -> Result<usize, std::io::Error> {
        let mut removed = 0;
        let mut entries = tokio::fs::read_dir(&self.base)
            .await
            .map_err(|error| with_path_context(error, "list sandbox cgroup base", &self.base))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| with_path_context(error, "read cgroup base entry", &self.base))?
        {
            let name = entry.file_name();
            if !is_sandbox_cgroup_name(&name.to_string_lossy()) {
                continue;
            }
            if tokio::fs::remove_dir(entry.path()).await.is_ok() {
                removed += 1;
            } else {
                warn!(
                    path = %entry.path().display(),
                    "orphan sweep could not remove a sandbox cgroup"
                );
            }
        }
        Ok(removed)
    }
}

/// A live per-action sandbox cgroup. Dropping it best-effort removes the
/// directory; [`CgroupGuard::release`] does the same with logging.
#[derive(Debug)]
pub(crate) struct CgroupGuard {
    path: PathBuf,
}

impl CgroupGuard {
    fn apply_limits(&self, memory_bytes: i64) -> Result<(), std::io::Error> {
        // Synchronous writes on purpose: the values are tiny, the directory was
        // just created by us, and keeping this straight-line keeps the
        // create-then-attach window minimal.
        let hard_limits = std::fs::write(self.path.join("memory.max"), memory_bytes.to_string())
            .and_then(|()| std::fs::write(self.path.join("pids.max"), PIDS_LIMIT.to_string()))
            .and_then(|()| std::fs::write(self.path.join("cpu.max"), CPU_MAX));
        if let Err(error) = hard_limits {
            return Err(with_path_context(error, "apply sandbox cgroup limits", &self.path));
        }
        // Swap is disabled, matching the container backend's
        // `memory_swap == memory`. Hosts without swap accounting still accept
        // the plain memory limit, so this write is best-effort.
        let _ = std::fs::write(self.path.join("memory.swap.max"), "0");
        Ok(())
    }

    /// Moves a process (and, transitively, its future children) into the
    /// cgroup. Must happen right after spawn so the untrusted tree never runs
    /// unbounded.
    pub(crate) async fn attach(&self, pid: u32) -> Result<(), std::io::Error> {
        write_cgroup_file(&self.path.join("cgroup.procs"), &pid.to_string()).await
    }

    /// Reads the `oom_kill` counter after the action finished.
    pub(crate) async fn oom_kill_count(&self) -> Option<u64> {
        let contents = tokio::fs::read_to_string(self.path.join("memory.events")).await.ok()?;
        parse_oom_kill(&contents)
    }

    /// Removes the cgroup directory. The process tree must already be gone.
    pub(crate) async fn release(self) {
        // A tiny retry absorbs the short window where the kernel still
        // reports released-but-reaped processes inside the group.
        for _ in 0..3 {
            match tokio::fs::remove_dir(&self.path).await {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
        warn!(path = %self.path.display(), "sandbox cgroup removal failed; the orphan sweep will retry");
        // Forget the path so Drop does not race the retry loop above.
        self.forget().await;
    }

    /// Drop-neutrality helper: hands the directory over to the orphan sweep.
    async fn forget(mut self) {
        self.path = PathBuf::new();
    }
}

impl Drop for CgroupGuard {
    fn drop(&mut self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let _ = std::fs::remove_dir(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_sandbox_cgroup_name, parse_controller_list, parse_oom_kill, subtree_control_payload,
    };

    #[test]
    fn sandbox_cgroup_names_match_the_action_naming_scheme() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(is_sandbox_cgroup_name(&format!("pb-judge-{id}-compile")));
        assert!(is_sandbox_cgroup_name(&format!("pb-judge-{id}-run")));
        assert!(is_sandbox_cgroup_name(&format!("pb-judge-probe-{id}")));
        assert!(!is_sandbox_cgroup_name("pb-judge-probe"));
        assert!(!is_sandbox_cgroup_name("pb-judge-not-a-uuid-run"));
        assert!(!is_sandbox_cgroup_name(&format!("{id}-run")));
        assert!(!is_sandbox_cgroup_name("unrelated-group"));
    }

    #[test]
    fn controller_lists_parse_into_sets() {
        let enabled = parse_controller_list("cpuset cpu io memory hugetlb pids\n");
        assert!(enabled.contains("memory") && enabled.contains("pids") && enabled.contains("cpu"));
        assert_eq!(parse_controller_list("").len(), 0);
    }

    #[test]
    fn subtree_payload_only_enables_missing_controllers() {
        let all = parse_controller_list("cpu memory pids");
        assert_eq!(subtree_control_payload(&all), None);

        let none = parse_controller_list("");
        assert_eq!(subtree_control_payload(&none), Some("+cpu +memory +pids".to_owned()));

        let partial = parse_controller_list("cpu 10-slot memory");
        assert_eq!(subtree_control_payload(&partial), Some("+pids".to_owned()));
    }

    #[test]
    fn oom_kill_counter_is_extracted_from_memory_events() {
        let events = "anon 2\nfile 0\n\
                      large 0\nhuge 0\n\nseq 12\n\
                      kill 0\noom 1\noom_kill 3\noom_group 0\n";
        assert_eq!(parse_oom_kill(events), Some(3));
        assert_eq!(parse_oom_kill("anon 0\n"), None);
        assert_eq!(parse_oom_kill(""), None);
        // A non-numeric counter must not poison other parses.
        assert_eq!(parse_oom_kill("oom_kill NaN\noom_kill 7\n"), Some(7));
    }
}
