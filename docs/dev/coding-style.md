# Coding Style

This document defines coding rules for the Rust workspace and the existing Vue
frontend.

## General Rules

- Prefer simple, explicit code over clever abstractions.
- Organize business code by feature and keep dependency direction visible.
- Add comments for invariants, safety boundaries, and operational constraints;
  do not narrate syntax.
- Do not introduce public-network dependencies into runtime behavior.
- Treat official contest behavior as auditable and recoverable.
- Keep secrets out of source code, logs, test snapshots, and examples.
- Declare the stable toolchain channel and a minimum Rust version, and commit
  `Cargo.lock` for reproducible offline builds. Release images must record the
  exact compiler version used.

## Rust Backend

Backend stack:

- Rust 2024 edition on the repository-pinned stable toolchain.
- Tokio and Axum.
- SQLx with PostgreSQL.
- `redis` for Redis.
- `lapin` for RabbitMQ.
- AWS SDK for Rust for RustFS/S3-compatible storage.
- Serde for external wire formats.
- `tracing` for structured diagnostics.

### Workspace Boundaries

- `apps/api` owns HTTP routing, authentication extraction, application startup,
  and API-hosted background tasks.
- `apps/judge-worker` owns judge-task consumption and sandbox orchestration.
- `crates/domain` contains pure types, validation, calculations, and state
  transitions. It must not depend on Axum, SQLx, RabbitMQ, Redis, or S3.
- `crates/contracts` contains explicitly versioned AMQP and event payloads.
- Do not create one crate per entity or business feature. Add a crate only when
  it creates a real compilation, deployment, or dependency boundary.

### Types And Domain Rules

- Use newtypes such as `ContestId`, `SubmissionId`, and `JudgementId` instead of
  passing unrelated raw IDs through the system.
- Represent finite states and verdicts with enums. Parse unknown external values
  explicitly; do not silently map them to a default.
- Put state-transition rules on domain types or pure domain functions.
- Keep HTTP request/response types, database rows, and domain objects distinct
  when their invariants differ.
- Prefer borrowed inputs (`&str`, slices, references) when ownership is not
  required, but do not add lifetime complexity merely to avoid small clones at
  process boundaries.
- Use UTC timestamps at persistence and protocol boundaries.

### Errors

- Define recoverable library and domain errors with `thiserror`.
- Use `anyhow` only at binary startup, operator command, or task-supervision
  boundaries where callers do not need to match individual variants.
- Map errors to one stable API error envelope in the HTTP adapter.
- Never return SQL, object keys, credentials, internal paths, or raw dependency
  errors to clients.
- `unwrap`, `expect`, `panic`, `todo`, and `unimplemented` are forbidden in
  production request/task paths. A narrowly justified startup invariant may use
  `expect` with an actionable message.

### Async And Concurrency

- Never hold a synchronous mutex guard or database transaction across unrelated
  `.await` points.
- Use bounded channels and explicit backpressure.
- Supervise spawned tasks; an unobserved detached task is not a background-job
  strategy.
- Put CPU-heavy work and blocking filesystem/process operations behind
  `spawn_blocking` or a dedicated bounded worker pool.
- Apply explicit timeouts to database, Redis, RabbitMQ, S3, CUPS, and sandbox
  operations.
- Pass cancellation and shutdown signals to consumers and long-running tasks.

### HTTP And Application Code

- Handlers extract and validate input, authorize the request, call one
  application operation, and convert its result. They do not contain SQL or
  multi-step contest logic.
- Compose routers by feature under one typed application state.
- Use typed authentication/authorization extractors and explicit policy
  functions; do not scatter string role checks through handlers.
- Preserve the reviewed behavior in `docs/api/openapi.yaml` while migrating.
  Contract changes require an explicit compatibility decision and tests.
- Keep cookie-session authentication and CSRF protection for browser mutations;
  do not replace it with JWT without a separate architecture decision.

### Persistence And Messaging

- PostgreSQL is the source of truth; Redis entries must be rebuildable.
- Use SQLx parameter binding. Dynamic identifiers or sort clauses must come from
  closed enums, never directly from request strings.
- Keep transaction boundaries visible in application operations.
- Publish judge and other durable tasks through a transactional outbox. Do not
  perform an uncoordinated database-write/RabbitMQ-publish dual write.
- Consumers must be idempotent. ACK only after the durable result or retry/dead
  path is established.
- Traits are for meaningful replaceable boundaries or test seams. Do not create
  a repository trait for every table by convention.

## Frontend

Frontend stack:

- Vue 3.
- TypeScript.
- Vite.
- Element Plus as the default UI component library.

Frontend guidelines:

- Separate contestant, admin, screen, live, resolver, and awards routes clearly.
- Never render sensitive fields on public, screen, or live pages.
- Generate or validate typed API clients against `docs/api/openapi.yaml`.
- Ensure pages work in offline LAN without CDN resources.
- Vendor fonts, icons, editor assets, MathJax/KaTeX, and syntax-highlighting
  resources.
- Use on-demand Element Plus imports to keep the offline bundle small.
- Screen, live, and resolver pages may use custom full-screen layouts but must
  still avoid external resources.

## Database Migrations

- Use SQLx migrations for all schema changes in the Rust reset.
- Never edit an applied migration. Add a forward migration.
- Avoid destructive migrations after official data exists unless backup and
  restore have been tested.
- Add indexes for contest-scoped queries, especially submissions, judgements,
  scoreboards, print tasks, and balloon tasks.
- Store large objects in RustFS and store their hashes, versions, and metadata
  in PostgreSQL.
- Commit SQLx offline query metadata and verify it in CI.

## Judge

- Treat submitted source, test data archives, and compiler output as hostile.
- Use typed sandbox specifications; never construct a shell command from
  contestant-controlled strings.
- Do not mount the Docker socket or grant privileged permissions in official
  deployments.
- Keep compiler/runtime versions visible in diagnostics.
- Bound all input, output, log, archive, process, and execution sizes.
- Clean work directories through an RAII guard so early returns cannot skip
  cleanup.

## Tests And Quality Gates

- Keep pure domain tests deterministic and free of infrastructure.
- Test SQL against PostgreSQL rather than substituting a different database.
- Add contract fixtures for AMQP messages, SSE events, and important API
  responses.
- Test retry, redelivery, duplicate delivery, timeout, cancellation, and
  shutdown behavior.
- Required checks are `cargo fmt --check`, Clippy with warnings denied, workspace
  tests, SQLx metadata verification when compile-time query macros are used, and
  dependency/license auditing.
- Unsafe Rust is disallowed by default. Any exception requires an architecture
  decision, a documented invariant, and focused tests.

## Documentation

- Update docs when architecture, deployment, security, or contest workflow
  changes.
- Keep operational docs executable: include inputs, commands, verification, and
  failure handling.
- Avoid documenting secrets or real credentials.
