# Local Development

This document defines local development for the Rust workspace. Commands are
run from the repository root unless stated otherwise. Production installation
uses the binary package described in `docs/ops/install.md`; the Compose files in
this section are development and rehearsal conveniences only.

## Required Toolchain

- The stable Rust toolchain selected by `rust-toolchain.toml`, at or above the
  workspace `rust-version`.
- Cargo and the repository-selected `cargo-nextest`, `cargo-sqlx`, audit, and
  license-checking tools.
- Node.js LTS and npm for the frontend.
- Docker Engine with Docker Compose for local infrastructure and judge sandbox
  development.
- PostgreSQL 16, Redis 7, RabbitMQ 3, and RustFS through Compose.

Do not require a globally installed Java or Gradle toolchain for the Rust
backend. Java remains installed only inside the contestant Java runtime image.

## Local Service Model

Run application code directly for fast rebuilds and run infrastructure through
Docker Compose.

```text
frontend/web          Vite development server
apps/api              Cargo binary; includes scheduler/outbox dispatcher
apps/judge-worker     Cargo binary or development container
data services         local Compose project
```

Local defaults must not be reused as official contest secrets. Production
PostgreSQL, Redis, RabbitMQ, object storage, sandbox, proxy, printing, and
observability services are provisioned and maintained by the deployer.

## Docker Integration Suite

Run the complete isolated Docker-backed suite with:

```text
scripts/test/docker-integration.sh
```

The script starts uniquely named PostgreSQL, Redis, RabbitMQ, and RustFS
containers, creates isolated buckets, declares the reviewed Judge topology,
runs all ignored API/Worker tests in queue-safe order, and removes its
containers on success or failure. Fixed Judge runtime images must already exist.

The same script is exercised by the scheduled/manual
`.github/workflows/docker-integration.yml` workflow. The regular PR workflow
intentionally runs only dependency-free tests; it does not silently claim to
cover these environment-backed scenarios.

## Development Data

Seed data should include:

- Admin user.
- Contest administrator user.
- Judge user if required by service authentication.
- Sample contest.
- Sample teams.
- Sample problems.
- Small test data package.

Seed data must not contain real contest credentials.

## Local Workflow

Recommended workflow after bootstrapping:

```text
Start local data services
  -> run database migrations
  -> start backend API
  -> start judge worker
  -> start frontend dev server
  -> submit sample solution
  -> verify scoreboard update
```

Create an ignored local Compose environment, replace every `CHANGE_ME` value,
and start only the development data services:

```bash
cp deploy/compose/.env.rust.example deploy/compose/.env.local
$EDITOR deploy/compose/.env.local
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml up -d
```

Copy the application environment template to the ignored root `.env`, change
the service URLs to their host-published `127.0.0.1` ports, then export it into
the shell. The API creates its configured object-storage buckets on startup.
Run the Rust processes directly:

```bash
cp .env.example .env
$EDITOR .env
set -a
. ./.env
set +a
PROJECT_BALLOON_API_BIND=127.0.0.1:18080 cargo run -p project-balloon-api

# In another shell, export .env as above before starting the Worker:
cargo run -p project-balloon-judge-worker
```

The Worker command performs strict RabbitMQ, RustFS, sandbox-socket, and runtime
image preflight before consuming. For the documented local Docker profile set
`JUDGE_CACHE_DIR` to a writable absolute host path and use uid/gid `1000:1000`.
Official deployments must instead use the rootless Podman socket, uid/gid
`10001:10001`, and `XCPC_SANDBOX_RUNTIME=runsc` required by ADR-001.

PostgreSQL integration tests use SQLx temporary test databases and are ignored
by the dependency-free default test run. Point `DATABASE_URL` at a PostgreSQL
server whose configured user may create databases, then run:

```text
cargo test -p project-balloon-api --test bootstrap_postgres -- --ignored
```

To run every ignored PostgreSQL scenario, including problem statements and
contest configuration freezing:

```text
cargo test -p project-balloon-api --lib -- --ignored
```

SQLx creates an isolated database, applies `migrations/`, runs the test, and
removes the database afterward. Never point this command at credentials that
cannot safely create and drop temporary test databases.

The API runs embedded SQLx migrations by default. Set
`PROJECT_BALLOON_RUN_MIGRATIONS=false` only when migrations are managed by a
separate reviewed deployment step.

Important API environment variables:

| Variable | Development default | Purpose |
|---|---|---|
| `DATABASE_URL` | unset | PostgreSQL connection; set an explicit URL with local credentials |
| `PROJECT_BALLOON_API_BIND` | `127.0.0.1:8080` | API listening socket |
| `PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL pool bound |
| `PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | Pool backpressure timeout |
| `PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS` | `1000` | Readiness probe timeout |
| `PROJECT_BALLOON_RUN_MIGRATIONS` | `true` | Run embedded SQLx migrations at startup |
| `PROJECT_BALLOON_SESSION_TTL_SECONDS` | `43200` | Browser session lifetime |
| `PROJECT_BALLOON_SECURE_COOKIES` | `false` | Add the cookie `Secure` attribute; required in production |
| `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` | `false` | Permit the checked-in development CSRF secret; set `true` for local development only |
| `PROJECT_BALLOON_CSRF_SECRET` | development-only value | HMAC secret for CSRF tokens; startup refuses the development value unless the flag above is set, and refuses it outright with secure cookies |
| `PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED` | `true` | Claim and publish durable realtime outbox rows |
| `PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY` | `1024` | Per-process SSE broadcast buffer |
| `PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS` | `250` | Outbox polling interval |
| `PROJECT_BALLOON_REALTIME_LEASE_SECONDS` | `30` | Recovery lease for abandoned claims |
| `PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS` | `1000` | Initial failed-delivery backoff |
| `PROJECT_BALLOON_REALTIME_BATCH_SIZE` | `100` | Maximum rows claimed per poll |
| `PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS` | `8` | Delivery attempts before operator intervention |
| `PROJECT_BALLOON_REALTIME_REDIS_ENABLED` | `false` | Publish and subscribe SSE fanout through Redis |
| `REDIS_URL` | unset | Redis connection URL with ACL credentials when enabled |
| `PROJECT_BALLOON_REALTIME_REDIS_CHANNEL` | `xcpc:realtime:events` | Version-1 realtime Pub/Sub channel |
| `PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS` | `1000` | Initial subscriber reconnect delay; exponentially capped at 30 seconds |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` | `false` | Cache rendered scoreboard variants in Redis while keeping PostgreSQL authoritative |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS` | `30` | Expiry for revision-scoped scoreboard cache entries |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS` | `200` | Maximum Redis connect/read/write wait before PostgreSQL fallback |

Practice defaults are stored in PostgreSQL and can be changed by a super
administrator from `/admin/practice`: daily submissions `200`, concurrent
judging `3`, and source retention `365` days. The API exposes the active
practice workload through Prometheus metrics; source deletion is disabled for
pending or judging submissions and is retried through the object cleanup
runner.

Probe behavior:

- `GET /livez` verifies that the Rust process and HTTP runtime are alive without
  accessing dependencies.
- `GET /api/health` verifies PostgreSQL readiness and, when realtime Redis
  fanout is enabled, Redis connectivity. It returns HTTP 503 with
  `status: down` while a required dependency is unavailable.

Browser authentication workflow:

1. Call `GET /api/auth/csrf` with credentials enabled.
2. Copy the response token into `X-XSRF-TOKEN`.
3. Send that header, the `XSRF-TOKEN` cookie, and JSON credentials to
   `POST /api/auth/login`.
4. Continue sending the CSRF header for `POST`, `PUT`, `PATCH`, and `DELETE`
   requests. The session itself is held in the `HttpOnly` `PB_SESSION` cookie.

The development CSRF secret is a public value, so startup refuses it unless
`PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET=true` is set, and it remains unusable
with secure cookies. Set a deployment-specific secret before enabling
`PROJECT_BALLOON_SECURE_COOKIES=true`.

Local development may leave Redis fanout disabled and use the in-process Tokio
broadcast channel. Multi-instance deployments enable Redis on every API
replica. Dispatchers may also run on multiple replicas: PostgreSQL
`SKIP LOCKED` prevents duplicate claims, while Redis delivers the event to each
replica's local SSE hub. Polling remains the browser's state-recovery path; SSE
events are invalidation hints, not authoritative business records.

Start the frontend separately:

```text
cd frontend/web
npm ci
npm run dev
```

Vite listens on `http://127.0.0.1:5173` and proxies `/api` to
`http://127.0.0.1:18080` by default. Override the backend target when running
the API directly on another port:

```text
XCPC_API_PROXY_TARGET=http://127.0.0.1:8080 npm run dev
```

Stop local dependencies:

```bash
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml down
```

To deliberately reset all local dependency data, remove the Compose volumes:

```bash
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml down --volumes
```

## Configuration Rules

- Commit `.env.example` files only.
- Keep real `.env` files out of Git.
- Use fixed service names in local Compose so application configuration is stable.
- Keep browser requests same-origin through the Vite `/api` proxy; do not add a
  second frontend-only API base URL.
- Keep local test data small and deterministic.

## Useful Checks

Before opening a pull request or merging feature work, run:

```text
cargo fmt --all --check
cargo check-all
cargo lint
cargo test-all
cargo deny check

cd frontend/web
npm run typecheck
npm test
```

When compile-time SQLx query macros are introduced, also run
`cargo sqlx prepare --check --workspace` and commit the generated `.sqlx`
metadata. The current workspace uses runtime SQLx queries, so no metadata
directory is required yet.

Integration tests that require PostgreSQL, RabbitMQ, Redis, RustFS, or the
development sandbox must declare that requirement and fail with an actionable
message when the dependency is unavailable.
