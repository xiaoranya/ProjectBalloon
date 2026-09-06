# ADR-003: Non-Container Judge Sandbox Backend (bubblewrap)

## Status

Accepted as an optional backend (`SANDBOX_BACKEND=bwrap`). ADR-001 remains the
production/contest requirement: rootless Podman with the gVisor `runsc` OCI
runtime. The bubblewrap backend is for hosts where container engines are
undesirable or unavailable, and for development.

## Decision

The judge worker additionally ships a non-container sandbox backend for Linux.
It reuses the container backend's judgement semantics verbatim — verdict
ordering (`resource_verdict`), GNU-time marker parsing, wall-clock
multipliers, output comparison, archive handling — and replaces only the
isolation mechanics.

### Process Model

Every action (one compile, or one test-case run) is a single fresh `bwrap`
process:

```
bwrap --unshare-all --die-with-parent --new-session --cap-drop ALL \
      --clearenv --dev /dev --proc /proc --tmpfs /tmp \
      --setenv PATH/LC_ALL/HOME/TMPDIR ... \
      --ro-bind /usr /usr --ro-bind /etc /etc (plus /bin /sbin /lib /lib64 when present) \
      --bind <job work dir> /work \
      --ro-bind <JUDGE_GNU_TIME_PATH> /work/.pb-time \
      -- <argv>
```

- `--unshare-all` gives user, mount, IPC, PID, network, and UTS namespaces.
  The user namespace makes the rest usable without root: the worker runs as a
  dedicated non-root account and the sandbox maps that uid to namespaced root,
  mirroring the rootless-Podman uid mapping of the container backend.
- Because bwrap is the PID-namespace init, its exit (or SIGKILL at the
  wall-clock deadline) tears down every contestant process. There is no
  container to clean, no `/proc` scan, and no residue between actions.
- The measurement tool is bound read-only at `/work/.pb-time`, inside the
  writable `/work` mount but itself read-only: the contestant can neither
  replace nor remove it, and it does not depend on the host layout.

### Resource Limits

Per-action cgroup v2 groups are created under `JUDGE_CGROUP_BASE` (a
delegated, empty base directory): `memory.max` (compile: max(run limit, 1 GiB);
run: task limit), `memory.swap.max=0`, `pids.max=64`, `cpu.max=1 core` —
mirroring the container backend's HostConfig. OOM is detected via the
`memory.events` `oom_kill` counter. Leftover cgroups are reclaimed by the
orphan sweep.

When cgroup v2 delegation is unavailable, `JUDGE_CGROUP_REQUIRED=false`
degrades to rlimit + post-hoc GNU-time enforcement with a startup warning;
`true` (default) fails the worker start instead.

### Configuration

- `SANDBOX_BACKEND=docker|bwrap` (default `docker`)
- `JUDGE_BWRAP_PATH` (default `/usr/bin/bwrap`)
- `JUDGE_GNU_TIME_PATH` (default `/usr/bin/time`)
- `JUDGE_CGROUP_BASE`, `JUDGE_CGROUP_REQUIRED`

Preflight verifies bwrap executability, a live namespace smoke test
(unprivileged user namespaces enabled), every judge-language toolchain binary
on the host PATH, GNU time, and cgroup delegation.

## Consequences

- No container engine dependency; per-action isolation cost drops to a
  fork/unshare (~10 ms), and there is no container lifecycle to pool.
- Safety level is below gVisor: untrusted code faces the host kernel. This is
  the same trade-off as mainstream OJ sandboxes (isolate and peers). For
  adversarial contests, prefer ADR-001.
- Judge hosts must install the toolchain (`gcc`, `g++`, `javac`, `python3`,
  `go`, `rustc`), GNU time, and bubblewrap; version pinning moves from images
  to host configuration.
- Unprivileged user namespaces must be enabled on the host; preflight fails
  with remediation hints when a distribution restricts them.
