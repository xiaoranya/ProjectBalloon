# Local Development

This document defines the target local-development shape for the Rust reset.
Commands are run from the repository root unless stated otherwise.

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

## Profiles

Expected profiles:

- `local`: developer machine with local Compose dependencies.
- `test`: automated tests.
- `offline`: packaged deployment configuration.
- `contest`: official contest hardening profile.

Local defaults must not be reused as official contest secrets.

## Docker Integration Suite

Run the complete isolated Docker-backed suite with:

```text
scripts/test/docker-integration.sh
```

The script starts uniquely named PostgreSQL, Redis, RabbitMQ, and RustFS
containers, creates isolated buckets, declares the reviewed Judge topology,
runs all 47 ignored API/Worker tests in queue-safe order, and removes its
containers on success or failure. Fixed Judge runtime images must already exist.

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

Current local dependency command:

```text
scripts/dev/start-local.sh
```

This starts the API at `http://127.0.0.1:18080`. Start the frontend separately:

During the document-first reset this script may still reference the archived
implementation. Before Rust application work is considered bootstrapped, it
must be changed to start only data services and the Rust binaries.

Target direct-development commands:

```text
cargo run -p project-balloon-api
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
| `DATABASE_URL` | `postgres://xcpc:xcpc@127.0.0.1:5432/xcpc` | PostgreSQL connection |
| `PROJECT_BALLOON_API_BIND` | `127.0.0.1:8080` | API listening socket |
| `PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL pool bound |
| `PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | Pool backpressure timeout |
| `PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS` | `1000` | Readiness probe timeout |
| `PROJECT_BALLOON_RUN_MIGRATIONS` | `true` | Run embedded SQLx migrations at startup |
| `PROJECT_BALLOON_SESSION_TTL_SECONDS` | `43200` | Browser session lifetime |
| `PROJECT_BALLOON_SECURE_COOKIES` | `false` | Add the cookie `Secure` attribute; required in production |
| `PROJECT_BALLOON_CSRF_SECRET` | development-only value | HMAC secret for CSRF tokens; replace in every deployment |
| `PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED` | `true` | Claim and publish durable realtime outbox rows |
| `PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY` | `1024` | Per-process SSE broadcast buffer |
| `PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS` | `250` | Outbox polling interval |
| `PROJECT_BALLOON_REALTIME_LEASE_SECONDS` | `30` | Recovery lease for abandoned claims |
| `PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS` | `1000` | Initial failed-delivery backoff |
| `PROJECT_BALLOON_REALTIME_BATCH_SIZE` | `100` | Maximum rows claimed per poll |
| `PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS` | `8` | Delivery attempts before operator intervention |
| `PROJECT_BALLOON_REALTIME_REDIS_ENABLED` | `false` | Publish and subscribe SSE fanout through Redis |
| `REDIS_URL` | `redis://127.0.0.1:6379/` | Redis connection URL; may contain ACL credentials |
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

The development CSRF secret is deliberately unusable with secure cookies. Set
a deployment-specific secret before enabling
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

```text
scripts/dev/stop-local.sh
```

Reset local dependency volumes:

```text
scripts/dev/reset-local.sh
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
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo sqlx prepare --check --workspace
cargo deny check

cd frontend/web
npm run typecheck
npm test
```

Integration tests that require PostgreSQL, RabbitMQ, Redis, RustFS, or the
development sandbox must declare that requirement and fail with an actionable
message when the dependency is unavailable.
