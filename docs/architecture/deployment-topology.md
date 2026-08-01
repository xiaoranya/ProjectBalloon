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
| `backup-01` | Backup storage, image backup, standby services | Stores backups and binary package copies |

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

## Binary Release Package Mapping

Source repository directories map to the release package as follows:

| Source | Binary package output |
|---|---|
| Rust release binaries | `bin/` |
| Vue production build | `web/` |
| Judge Runtime images | `judge-images/*.tar` |
| systemd units and environment templates | `systemd/`, `config/` |
| Nginx template | `nginx/` |
| backup scripts | `scripts/backup/` |
| deployment script libraries | `scripts/lib/` |

Generated release package shape:

```text
project-balloon-vX.Y.Z-linux-amd64/
  bin/
  web/
  judge-images/
  systemd/
  config/
  nginx/
  scripts/backup/
  docs/
  install.sh
  PACKAGE-SHA256SUMS
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

## Binary Deployment Flow

```text
Copy binary release archive to target host
  -> install external PostgreSQL, Redis, RabbitMQ, and RustFS
  -> run install.sh --role api --no-start on app/gateway hosts
  -> install Docker/Podman on judge hosts
  -> run install.sh --role worker --skip-nginx --no-start on judge hosts
  -> fill in /etc/project-balloon/project-balloon.env
  -> run the corresponding role again to import images and start services
  -> bootstrap the first administrator
  -> run health checks and verify backups
```

The binary model keeps stateful services outside the application package. API
and Worker processes can run on separate hosts, while Judge Workers retain
access to their local Docker/Podman sandbox socket and runtime images.

The flow must be repeatable. Scripts should fail fast and print the failed step clearly.
