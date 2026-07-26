# Data Storage

This document defines storage ownership and first-version persistence rules.

## Storage Responsibilities

| Store | Responsibility |
|---|---|
| PostgreSQL | Authoritative relational business data |
| Redis | Rebuildable cache, counters, and hot real-time data |
| RabbitMQ | Judge task transport and retry/dead-letter queues |
| RustFS | Large file objects and generated artifacts |

PostgreSQL is the source of truth for contest results. Redis and RabbitMQ must never be the only durable copy of final contest state.

## PostgreSQL Core Tables

Initial table groups:

Identity and contest setup:

- `users`
- `teams`
- `team_members`
- `contests`
- `contest_teams`
- `problems`
- `contest_problems`

Judging and scoreboard:

- `submissions`
- `judgements`
- `runs`
- `contest_scoreboard_cells`
- `contest_scoreboard_rows`
- `scoreboard_snapshots`

The two live scoreboard projection tables are PostgreSQL-authoritative and are rebuilt per affected
team/problem inside the same transaction that accepts a final Judge result. The projection counts
WA, TLE, MLE, RE, and OLE as 20-minute wrong attempts before the first AC; compile errors, system
errors, and cancellations do not add penalty. Snapshots remain separate immutable artifacts for
freeze, resolver, and award workflows.

Each scoreboard snapshot records its `PUBLIC` or `ADMIN` variant, optional group and participation
filters, a selector-local monotonic version, the complete serialized board, its SHA-256 digest, and
the creating staff user. PostgreSQL uniqueness and advisory-lock-backed allocation prevent duplicate
versions under concurrent creation. A database trigger rejects updates and deletes, so later Judge
results or rejudges can rebuild live projections without changing historical resolver or award input.

The live administrator board reads these projection tables. While a contest is between `freeze_at`
and `end_at`, the public board reconstructs the same ICPC cells from active final judgements whose
submission timestamp precedes `freeze_at`; this prevents a post-freeze result from mutating public
rankings without making Redis a source of truth.

When enabled, Redis stores complete rendered boards under revision-scoped keys. The revision is
read from PostgreSQL with the contest schedule and is advanced by database triggers for every
persisted input that changes standings or presentation. Cache entries also separate public/admin,
live/frozen/final phases, groups, and participation types. Reads and writes degrade to PostgreSQL
on Redis failure; old revisions expire by TTL and are never selected again.

Communication and on-site operations:

- `clarifications`
- `announcements`
- `print_requests`
- `balloon_tasks`
- `balloon_colors`

Resolver and awards:

- `resolver_runs`
- `resolver_snapshots`
- `resolver_events`
- `award_categories`
- `award_rules`
- `award_recipients`

Screen, live, and audit:

- `screen_instances`
- `screen_groups`
- `screen_commands`
- `broadcast_tokens`
- `audit_logs`

## RustFS Object Categories

RustFS is used as the S3-compatible object storage service for offline deployments. Application code should depend on object-storage semantics and S3-compatible APIs rather than implementation-specific behavior where practical.

RustFS stores:

- Problem attachments.
- Test data.
- Submission source files.
- Compile logs.
- Judge logs.
- Export files.
- Print PDFs.

Suggested bucket layout:

```text
problems/
testdata/
submissions/
judge-logs/
exports/
prints/
backups/
```

Object keys should include contest ID or problem ID where applicable. Uploaded test data should include hash and version metadata.

The Rust API currently standardizes these keys:

```text
problems/{problemId}/attachments/{sha256}/{uuid}-{safeFilename}
problems/{problemId}/testdata/v{version}/{uuid}.zip
submissions/{contestId}/{teamId}/{uuid}.{languageExtension}
```

Bucket names are deployment configuration and are never embedded in database
object keys or returned through public APIs.

## Data Versioning

Objects and rows that affect official contest results must be versioned or frozen.

Version-sensitive data:

- Problem statements.
- Test data packages.
- Time and memory limits.
- Language configuration.
- Contest problem aliases and order.
- Balloon colors.
- Scoreboard snapshots.
- Resolver snapshots.

After contest configuration freeze, changes should require privileged operation and audit logs.

## Schema Migrations

Fresh Rust installations start from
`migrations/20260719000000_initial_baseline.sql`. The baseline represents the
effective schema of the previous migration history; it does not copy the
previous framework's entity model.

The API embeds SQLx migrations into its executable and applies them before
accepting traffic. Applied migrations are immutable. The Rust release supports
fresh installations only; an existing installation from the previous
implementation must not replay the fresh-install baseline. Any historical-data
transfer is a separate export/import project, not an upgrade guarantee.

## Audit Requirements

Audit logs should record:

- Actor user ID and role.
- Operation type.
- Target resource.
- Previous and new values where reasonable.
- Request IP.
- Timestamp.
- Result status.

Critical audited actions:

- Role and account changes.
- Contest time changes.
- Problem/test data changes.
- Rejudge actions.
- Scoreboard snapshot generation.
- Resolver snapshot generation.
- Award freeze.
- Print task cancellation/rejection.
- Backup and restore operations.

## Realtime Outbox

Realtime notifications derived from committed business changes are first
stored in PostgreSQL `realtime_outbox`. A business transaction must never
publish directly to Redis before commit or rely on a best-effort after-commit
callback.

Each row contains a unique event ID, contest, event type, audience scope,
versioned JSON payload, availability time, attempt count, and delivery state.
The dispatcher may retry safely and Redis remains a fanout transport rather
than the durable source of truth.

Pending rows are expected while the dispatcher is stopped. Operational
readiness and monitoring must distinguish a small transient backlog from a
stalled or exhausted retry queue.

Claims use a time-bounded `PUBLISHING` lease. A replacement dispatcher changes
an expired claim to `FAILED` and retries it, so a crash can duplicate an
invalidation event but cannot silently strand it. Browser consumers use
`event_id` for deduplication and continue periodic REST polling for convergence.
TEAM rows require a `team_id`; a database constraint rejects both unaddressed
TEAM events and recipient IDs attached to broader scopes.

Redis Pub/Sub is the shared delivery transport for multiple API replicas, but
it is not a durable queue. The dispatcher confirms an Outbox row only after
Redis accepts `PUBLISH`; connection failures move the row to `FAILED` for
bounded exponential retry. Each Redis envelope carries an origin instance UUID
so the publishing replica can perform immediate local delivery without
processing its own Pub/Sub echo.

## Backup Scope

Official contest backups must include:

- PostgreSQL dump or base backup.
- RustFS object data and metadata.
- Deployment configuration.
- Compose files and image version manifest.
- Generated exports if needed.

Redis backup is optional for recovery because Redis state should be rebuildable. RabbitMQ queue state matters during a live contest; backup strategy should focus on graceful stop or drain before maintenance.
