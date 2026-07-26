# Offline Install

The active Rust deployment uses three Compose projects on the shared
`project-balloon` network:

- `data`: PostgreSQL, Redis, RabbitMQ, and RustFS;
- `app`: Axum API, Rust Judge Worker, and the Nginx-served Vue application;
- `monitor`: Prometheus, Grafana, Loki, Promtail, and exporters.

The nested `ProjectBalloon/` Java tree is not used by this procedure.

## Host prerequisites

Install these from trusted offline media before running the platform scripts:

- Docker Engine and the Docker Compose plugin;
- `curl`, `gzip`, and GNU coreutils;
- AWS CLI v2 for PostgreSQL/RustFS backup and restore.

The release package currently contains application and middleware image archives.
Packaging the host OS dependencies themselves remains release-engineering work
because the exact RPM/DEB closure depends on the selected distribution.

## Build an offline package

On a connected release machine, pull the pinned middleware images and then run:

```text
scripts/release/build-images.sh
scripts/release/export-images.sh
scripts/release/make-offline-package.sh
```

`build-images.sh` builds the API, Worker, web, and four Judge runtime images.
`export-images.sh` writes Docker archives plus `images/SHA256SUMS`.
`make-offline-package.sh` verifies those archives and creates
`dist/project-balloon-offline-<version>` without copying runtime secrets.

## Install on the contest host

From the unpacked package root:

```text
scripts/deploy/load-images.sh
scripts/deploy/install.sh
$EDITOR deploy/compose/.env.rust
scripts/deploy/install.sh
scripts/deploy/start.sh all
scripts/deploy/healthcheck.sh all
```

The first `install.sh` run creates `deploy/compose/.env.rust` with mode `0600`
and exits so the operator can replace every `CHANGE_ME` value. The second run
reasserts mode `0600` and validates all three Compose projects. The populated env file is ignored by Git
and excluded from offline packages and configuration backups.

The default web endpoint is `http://<host>:8088`; `/api/health` is the dependency
readiness endpoint and `/livez` is process liveness.

## Routine control

All lifecycle commands accept `data`, `app`, `monitor`, or `all`:

```text
scripts/deploy/start.sh <role>
scripts/deploy/stop.sh <role>
scripts/deploy/restart.sh <role>
scripts/deploy/status.sh <role>
scripts/deploy/healthcheck.sh <role>
```

`all` starts data first and stops it last. Start commands use `--no-build`, so an
air-gapped host cannot accidentally attempt a source build or dependency fetch.

## Judge host warning

The current `app` Compose defaults to the host Docker socket and is the reviewed
single-host development/rehearsal topology. For production, set
`XCPC_SANDBOX_SOCKET_HOST`, `XCPC_SANDBOX_SOCKET`, `XCPC_SANDBOX_SOCKET_GID`, and
`XCPC_SANDBOX_RUNTIME=runsc` in `.env.rust`. `install.sh` then fails unless the
runtime is registered and can start a fixed Judge image. A rootless
Podman-compatible API, matching socket/cache mounts, and execution of the
sandbox and broker-restart tests are still required; do not claim production
acceptance from a sibling-Docker rehearsal.

## Backup and restore

Create an atomic PostgreSQL and RustFS backup with:

```text
scripts/backup/backup.sh
```

A restore replaces the configured database and mirrors backed-up objects with
`--delete`; it therefore requires an explicit acknowledgement:

```text
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  scripts/backup/restore.sh backups/project-balloon-<timestamp>
```

After restore, start `app` and `monitor`, run the health check, and verify contest,
submission, scoreboard, Resolver, award, balloon, and printing state.
