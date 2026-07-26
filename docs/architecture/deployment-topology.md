# Deployment Topology

This document defines the standard offline LAN deployment shape.

## Host Roles

| Host | Services | Notes |
|---|---|---|
| `gateway-01` | Nginx | Single LAN entrypoint; can be backed up by manual failover initially |
| `app-01` | Web static files, API backend | Primary API node |
| `app-02` | API backend, Judge Scheduler | Second API node and scheduler host |
| `data-01` | PostgreSQL, Redis, RabbitMQ, RustFS | Stateful services; separate from judge workers |
| `judge-01` to `judge-N` | Judge Worker | CPU-intensive and isolated execution hosts |
| `backup-01` | Backup storage, image backup, standby services | Stores backups and offline package copies |

## Network Zones

Recommended zones:

- Public LAN access: contestants, administrators, screens, live production machines.
- Application zone: gateway and application hosts.
- Data zone: PostgreSQL, Redis, RabbitMQ, RustFS.
- Judge zone: judge workers and sandbox runtime.
- Operations zone: backup and monitoring access.

Only required ports should be exposed between zones. Database and queue ports should not be reachable by contestant machines.

## Service Placement

The production target should avoid co-locating judge workers with the database. Judge workload is CPU, memory, process, and disk intensive, and also has the highest security risk. The active Rust Compose file is a single-host rehearsal topology and intentionally mounts the Docker socket.

Acceptable development or rehearsal deployments may use fewer machines, but production documentation and scripts should keep role separation clear.

## Offline Package Mapping

Source repository directories map to the release package as follows:

| Source | Offline package output |
|---|---|
| `deploy/compose/` | `compose/` |
| `deploy/config/` | `config/` |
| `scripts/deploy/` | `scripts/` |
| `scripts/backup/` | `scripts/` |
| `docs/ops/` | `docs/` |
| built Docker images | `images/*.tar` |
| offline installers | `packages/` |

Generated release package shape:

```text
xcpc-platform-offline-vX.Y.Z/
  images/
  packages/                 # Docker, Compose, Podman, runsc, AWS CLI + checksums
  deploy/
    compose/
    config/
  judge/runtimes/runtime-images.properties
  frontend/web/dist/
  scripts/
  docs/
  VERSION
```

## Compose Files

The active Rust workspace currently has three Compose projects:

- `rust-app.docker-compose.yml` (API, Worker, and web)
- `data.docker-compose.yml`
- `../observability/compose.yml`

The five-role layout below remains the production separation target; its
rootless Podman/runsc and gateway packaging are not yet accepted.

Compose images must use fixed version tags. `latest` is not allowed for official releases.

## Configuration Rules

- Commit only `.env.example` files and non-secret templates.
- Real `.env` files are generated or copied during deployment and must not be committed.
- Service passwords, tokens, RustFS keys, database credentials, and live tokens are secrets.
- Nginx, Prometheus, Grafana, Loki, RabbitMQ, and RustFS templates live under `deploy/config/`.

## Deployment Flow

```text
Copy offline package to target host (single-host default)
  -> install Docker Engine and Docker Compose Plugin from approved offline media
  -> load image tar files (load-images.sh)
  -> seed .env.rust and validate secrets (install.sh)
  -> fill in secrets in deploy/compose/.env.rust
  -> start services via docker compose (start.sh)
  -> run health check (healthcheck.sh)
```

The single-host compose model runs every role on one host from the package
root; the earlier `/opt/xcpc` + systemd-unit layout is removed (see
`docs/ops/install.md#upgrade-from-the-systemd-model`). Multi-host role
separation remains available as an advanced configuration via the explicit
PostgreSQL, Redis, RabbitMQ, and object-storage host/endpoint variables.

The flow must be repeatable. Scripts should fail fast and print the failed step clearly.
