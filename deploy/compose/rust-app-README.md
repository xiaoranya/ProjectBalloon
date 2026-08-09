# Rust Application Compose

This is a compatibility stack for local development, integration testing, and
single-host rehearsal. Official deployments use the binary package and
user-managed external services described in `docs/ops/install.md`; this Compose
stack is not a production deployment method.

`rust-app.docker-compose.yml` builds and runs the Rust API, Rust Judge Worker,
and Vue frontend. PostgreSQL, Redis, RabbitMQ, RustFS, and the monitoring
stack are expected to be on the existing external `project-balloon` network.

The API and Judge images use release binaries built with `--locked`. The Judge
container receives the Docker socket because sandbox execution is an explicit
host capability; restrict access to this rehearsal stack and do not expose the
socket through TCP. This sibling-Docker setup does not satisfy the production
sandbox requirements in `docs/architecture/ADR-001-production-judge-sandbox.md`.

Required variables include `DATABASE_URL`, CSRF secret, RabbitMQ URL, object
storage endpoint/credentials, and matching worker/runtime image configuration.
Create the Judge cache directory on the Docker host with the UID/GID configured
by `XCPC_SANDBOX_USER`; it is mounted at the identical absolute path because
the host Docker daemon resolves sandbox bind mounts outside the worker
container.
Run a configuration-only check before starting:

```text
cp deploy/compose/.env.rust.example deploy/compose/.env.rust
$EDITOR deploy/compose/.env.rust
sudo install -d -o 1000 -g 1000 /var/lib/project-balloon/judge-cache
docker compose --env-file deploy/compose/.env.rust -f deploy/compose/data.docker-compose.yml up -d
docker compose --env-file deploy/compose/.env.rust -f deploy/compose/rust-app.docker-compose.yml config --quiet
```
