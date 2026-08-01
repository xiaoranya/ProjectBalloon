# ADR-001: Production Judge Sandbox Runtime

## Status

Accepted for the production Compose profile.

## Decision

Production Judge workers run as uid/gid `10001` and use the Docker-compatible
API of a rootless Podman service owned by the same dedicated host account. Podman
must launch submission containers with the `runsc` (gVisor) OCI runtime. The
Worker never mounts `/var/run/docker.sock`, never runs privileged, and has no
root-equivalent container-engine access.

The Worker cache is a host directory mounted at the identical absolute path in
the Worker container. This is required because the OCI daemon resolves the
`/work` bind source in the host namespace. A named volume is not valid for this
architecture.

Sibling Docker remains a development-only exception in
`local.docker-compose.yml`. It is not an official contest deployment profile.

## Required Host Preparation

1. Create a dedicated system account with uid/gid `10001` and no interactive
   login.
2. Install Podman and gVisor (`runsc`) from offline, checksummed packages.
3. Configure rootless Podman for that account and verify `runsc` appears as an
   available OCI runtime.
4. Create `XCPC_JUDGE_CACHE_DIR` owned by `10001:10001` with mode `0700`.
5. Start the dedicated rootless Podman API service and set
   `XCPC_SANDBOX_SOCKET=/run/xcpc-judge/podman.sock`.
6. Load the four runtime images (C, C++, Java, and Python) into the rootless
   Podman image store.

Preflight must fail if the socket, runtime images, cache directory, S3 buckets,
or RabbitMQ are unavailable. A release candidate is not contest-ready until the
Docker-tagged sandbox tests pass against this exact profile.

## Security Properties

- Submission containers have no network, a read-only root filesystem,
  `no-new-privileges`, a fixed non-root user, and CPU/memory/PID/output limits.
- gVisor provides a userspace-kernel boundary around untrusted submissions.
- Compromise of the Worker or Podman API is limited to the dedicated rootless
  account and isolated Judge host, rather than host root or the data zone.
- Judge hosts remain separate from PostgreSQL, Redis, RabbitMQ, and RustFS hosts
  for official contests.
