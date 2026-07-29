# Rust API Implementation Notes

`openapi.yaml` remains the compatibility baseline captured from the previous
backend. The notes below describe reviewed Rust behavior that is intentionally
more explicit than the generated baseline.

## Runtime OpenAPI

The Rust API now generates its runtime OpenAPI 3.1 contract from Utoipa
annotations:

| Path | Purpose |
|---|---|
| `GET /api/openapi.json` | Machine-readable generated contract |
| `GET /api/docs` | Offline-capable Swagger UI using vendored assets |

The generated contract currently covers all 171 Rust operations: process/readiness
health, all five authentication endpoints, the eight contest core endpoints,
the fourteen team and contest-roster endpoints, sixteen problem catalog and
test-data endpoints, seventeen submission,
rejudge, batch-rejudge, and export endpoints, the complete announcement
workflow, six scoreboard and snapshot endpoints, seven printing endpoints,
seven balloon endpoints, seven clarification endpoints, thirteen Resolver
endpoints, eighteen award, presentation, host-script, and certificate
endpoints, contest Judge queue status, four Staff Account and two Contest
Administrator Scope endpoints, one Audit Log query, and three realtime SSE
endpoints, plus three asynchronous submission-export task endpoints. It also exposes the
unauthenticated `/metrics` Prometheus exposition endpoint alongside the JSON health probes. The
contract documents the `PB_SESSION` session cookie plus the `XSRF-TOKEN` cookie and matching
`X-XSRF-TOKEN` header as separate security schemes; mutations require all
applicable schemes. Login requires the CSRF cookie and header but no existing
session; logout and password changes require all three. Contract tests verify
unique operation IDs, matching path parameters, authentication security
combinations, RFC3339 date-time schemas, and the served JSON endpoint.

This runtime contract is deliberately separate from `docs/api/openapi.yaml`.
The YAML file is the frozen Java compatibility input; reviewed Rust operations
move into the generated contract incrementally until all current Axum routes
are represented.

## P2 Submission Similarity Foundation

New submissions persist a SHA-256 `source_fingerprint` generated after removing
comments and formatting whitespace while preserving string and character literals.
Contest managers can query
`GET /api/admin/contests/{contestId}/submission-similarity` with optional
`problemId`, `language`, and `minGroupSize` filters. Results are grouped by
contest, problem, language, and normalized fingerprint, and contain only
submission/team IDs and counts. This first P2 slice detects exact normalized
duplicates without changing judging or exposing source code. The same migration
stores a 64-bit SimHash over normalized five-token shingles; contest managers can
query `/api/admin/contests/{contestId}/submission-similarity/pairs` with a bounded
similarity threshold (50--100 percent) to review cross-team approximate matches.
The result is capped and excludes same-team pairs; approximate matches are
candidate evidence only and never automatically trigger disciplinary action.
For pre-migration submissions, an authenticated CSRF-protected backfill endpoint
processes at most 1,000 rows per request, re-downloads each source, verifies its
authoritative SHA-256, and writes signatures only after verification.

## P2 Presentation Templates

Screen and live presentation configurations select one of four validated visual
templates: `DEFAULT`, `CINEMATIC`, `MINIMAL`, or `SPLIT`. Existing configurations
are migrated to `DEFAULT`; both operator consoles persist the selection, and the
OBS live views apply the chosen layout together with the configured accent color.
Unknown template identifiers are rejected by both the API and a database check
constraint so public presentation pages cannot be used to inject markup or CSS.

## Object Storage Orphan Compensation

The object-storage adapter now supports paginated bucket enumeration. The
cleanup module runs an hourly idempotent orphan scan that compares only the
application-owned prefixes (`problems/`, `submissions/`, and `prints/`) with
authoritative database references, then persists unmatched keys as
`ORPHAN_SCAN` cleanup tasks. Existing leases, retries, and the unique
`(bucket, object_key)` constraint make a scan safe to repeat and allow a
temporary storage failure to be compensated later by the cleanup runner.
Unknown prefixes are never touched, newly listed objects remain protected by
a fifteen-minute grace period, and adapters without listing support keep the
safe deletion-only behavior.

The same scan also performs reverse reconciliation. Durable database references
that are absent from a complete bucket listing are recorded in
`object_storage_integrity_findings`; a later successful scan marks a finding
resolved when the object is restored or its database reference is removed.
Unresolved counts are exposed in readiness JSON, Prometheus, and the
administrator health dashboard. Missing references are never repaired or
deleted automatically because the database does not contain enough information
to reconstruct their contents safely.

## Staff Accounts

All endpoints require the `SUPER_ADMIN` role and a completed mandatory password
change:

| Method | Path | Behavior |
|---|---|---|
| `GET` | `/api/admin/staff-accounts` | Lists non-team accounts ordered by username |
| `POST` | `/api/admin/staff-accounts` | Creates an enabled staff account with mandatory password change |
| `PATCH` | `/api/admin/staff-accounts/{userId}` | Updates display name, staff type, or enabled state |
| `POST` | `/api/admin/staff-accounts/{userId}/reset-password` | Replaces the password and revokes all target sessions |

Pagination accepts `page` from zero and `size` from 1 through 100. The only
accepted sort expression is `username,asc`; ordering is fixed rather than
interpolated into SQL.

Important stable errors:

| HTTP | Code | Meaning |
|---|---|---|
| 400 | `VALIDATION_FAILED` | Invalid body, pagination, staff type, or password |
| 401 | `NOT_AUTHENTICATED` | Missing or expired session |
| 403 | `PASSWORD_RESET_REQUIRED` | The actor must change their own password first |
| 403 | `FORBIDDEN` | The actor is not a super administrator |
| 404 | `STAFF_ACCOUNT_NOT_FOUND` | The target does not exist or is a team account |
| 409 | `USERNAME_TAKEN` | The normalized username already exists |
| 409 | `SELF_ACCESS_CHANGE_FORBIDDEN` | The actor attempted to remove their own super-admin access |
| 409 | `LAST_SUPER_ADMIN` | The mutation would leave no enabled super administrator |
| 409 | `STAFF_ROLE_NOT_CONFIGURED` | Required built-in role data is missing |

Password hashes and raw session tokens never appear in responses. Successful
mutations write `STAFF_ACCOUNT_CREATED`, `STAFF_ACCOUNT_UPDATED`, or
`STAFF_PASSWORD_RESET` to `audit_logs`.

## Contest Administrator Scopes

Both endpoints require `SuperAdminContext`:

| Method | Path | Behavior |
|---|---|---|
| `GET` | `/api/admin/contest-admins` | Lists contest administrators and sorted contest IDs |
| `PUT` | `/api/admin/contest-admins/{userId}/contests` | Atomically replaces the administrator's scope |

Input IDs must be positive, are deduplicated and sorted, and are limited to
1,000 entries. Every referenced contest must exist and must not be soft
deleted. Validation, deletion, bulk insertion, and
`CONTEST_ADMIN_SCOPE_UPDATED` audit insertion share one transaction. A failed
validation leaves the previous scope unchanged.

Stable not-found errors are `CONTEST_ADMIN_NOT_FOUND` and `CONTEST_NOT_FOUND`.

## Audit Log Query

`GET /api/admin/audit-logs` requires `SuperAdminContext` and supports:

- exact `actorUserId`;
- case-insensitive substring `action`;
- case-insensitive exact `result`;
- inclusive RFC3339 `from` and `to`;
- page sizes from 1 through 100.

The fixed ordering is `createdAt,desc` with `id` as a deterministic tiebreaker.
Percent, underscore, and backslash characters in `action` are escaped before
the parameterized `LIKE` query, so user input cannot become an unintended
wildcard. An inverted time range returns `VALIDATION_FAILED`.

## Contest Core

This slice implements:

| Method | Path | Authorization |
|---|---|---|
| `GET` | `/api/contests` | Optional authentication; response is visibility-scoped |
| `GET` | `/api/contests/{contestId}` | Optional authentication; inaccessible contests return 404 |
| `POST` | `/api/contests` | Completed-password `SUPER_ADMIN` |
| `PATCH` | `/api/contests/{contestId}` | `SUPER_ADMIN` or assigned `CONTEST_ADMIN` |
| `DELETE` | `/api/contests/{contestId}` | `SUPER_ADMIN` or assigned `CONTEST_ADMIN` |

Read visibility is intentionally evaluated in one database predicate:

- anonymous users see non-deleted `PUBLIC` contests;
- assigned teams see their contests plus public contests;
- contest administrators see assigned contests plus public contests;
- judge and operational read-all roles see every non-deleted contest;
- only super administrators may set `includeDeleted=true`.

Pagination is limited to 500 rows because the existing permission-management
screen requests that bound. Sort fields are allow-listed and translated to
fixed SQL fragments; values are never interpolated.

Creation accepts either no schedule or all of `startAt`, `freezeAt`, and
`endAt`, ordered as `startAt <= freezeAt <= endAt`. Patch requests may update
individual schedule fields by combining them with the stored schedule, but
schedule changes are rejected with `CONTEST_SCHEDULE_LOCKED` after the contest
leaves `DRAFT` or `FROZEN_CONFIG`.

Active contest names are protected by a PostgreSQL partial unique index. A
soft-deleted name can be reused. Updates increment `version`; creation, update,
and deletion write `CONTEST_CREATED`, `CONTEST_UPDATED`, and `CONTEST_DELETED`
in the same transaction.

Clone is implemented; restore of an archived contest remains a future slice.

## Contest Lifecycle And Extension

The lifecycle endpoint is
`POST /api/contests/{contestId}/transitions`. Allowed edges are defined in the
pure `project-balloon-domain` crate:

```text
DRAFT -> FROZEN_CONFIG -> RUNNING -> ENDED -> ARCHIVED
                              |
                              v
                            PAUSED
                              |
                              +-----> RUNNING
```

`RUNNING` may move directly to either `PAUSED` or `ENDED`.
`FROZEN_CONFIG` requires a complete, ordered schedule. The service locks the
contest row before evaluating the transition, so concurrent identical requests
produce exactly one success and the remaining requests receive
`CONTEST_TRANSITION_INVALID`.

`POST /api/contests/{contestId}/extensions` accepts `expectedEndAt` and
`newEndAt`. Extension is allowed only in `RUNNING` or `PAUSED`, requires the
stored end time to equal `expectedEndAt`, and requires the new value to be
later. Stable conflicts are:

- `CONTEST_EXTENSION_STATUS_INVALID`;
- `CONTEST_END_TIME_NOT_SET`;
- `CONTEST_EXTENSION_STALE`;
- `CONTEST_EXTENSION_NOT_LATER`.

Successful transitions and extensions increment `version` and write audit rows
in the business transaction. Extension also writes PUBLIC and STAFF
`CONTEST_EXTENDED` messages to `realtime_outbox`.

The API-hosted dispatcher claims available rows with `FOR UPDATE SKIP LOCKED`,
increments the attempt count, and gives every `PUBLISHING` row a lease.
Successful local fanout marks the row `PUBLISHED`; an expired lease is recovered
as `FAILED` and reclaimed. Delivery is therefore at least once, and consumers
deduplicate by the stable event UUID.

The Rust API preserves the existing browser routes:

- `GET /api/public/events/contests/{contestId}`;
- `GET /api/events/contests/{contestId}`;
- `GET /api/team/events/contests/{contestId}`.

Frames retain the version-1 Java/TypeScript shape. Public access uses normal
contest readability, staff access requires an approved operator role plus
contest readability, and team access resolves the authenticated team within
the contest. The current fanout channel is process-local; production
instances optionally bridge their local Tokio channels through Redis Pub/Sub.
The wire envelope remains compatible with Java:
`{originInstanceId, teamId, event}`. A source instance publishes locally and
ignores its own Redis echo; peer instances validate and forward the envelope.
Redis publication must succeed before the dispatcher marks an Outbox row
`PUBLISHED`.

## Team Accounts And Contest Rosters

Contestant identity is an explicit one-to-one `team_accounts` relation from an
authenticated user ID to a team ID. Authorization never depends on mutable
user or team display names.

The Rust API provides team CRUD, member CRUD, password reset, atomic batch
import, and contest-roster assignment under `/api/teams` and
`/api/contests/{contestId}/teams`. Team responses include their optimistic
`version` and account metadata; updates may send `expectedVersion` to reject
stale writes.

Super administrators may manage every team. Contest administrators may manage
a team only when every contest containing that team is inside their assigned
scope; teams not yet assigned to a contest remain super-administrator-only.

Batch imports accept 1–100 rows and require an `idempotencyKey`. PostgreSQL
transaction advisory locking serializes retries of the same key, and the whole
batch either commits or rolls back. Supplied passwords are hashed with Argon2
and are never returned by the API.

Roster changes are rejected after a contest reaches `ENDED` or `ARCHIVED`.
Every mutation records an audit entry in the business transaction. Roster
changes also enqueue STAFF events and a private TEAM event for the affected
team.

## Problem Bank Core

The first Rust problem-bank slice exposes super-administrator-only CRUD at
`/api/problems`. It validates lowercase kebab-case slugs, bounded positive
resource limits, language tags, and the closed P0 judge language set (`c`,
`cpp`, `java`, and `python`). Update requests require `expectedVersion` and
return `PROBLEM_VERSION_STALE` on concurrent modification.

Problem deletion is soft deletion and is rejected with
`PROBLEM_ASSIGNED_TO_CONTEST` while any contest assignment exists. PostgreSQL
enforces positive limits and active-only slug uniqueness. Creation, update, and
deletion write `PROBLEM_CREATED`, `PROBLEM_UPDATED`, and `PROBLEM_DELETED`
audit events in the same transaction.

The team-facing projection is provided through contest problem reads;
scoped administrators can manage persisted statements, attachments, and
immutable test-data versions through their dedicated endpoints.

Multilingual statements are upserted at
`PUT /api/problems/{problemId}/statements/{langCode}`. The API retains bounded
Markdown source for editing and returns HTML rendered by `pulldown-cmark` then
sanitized by `ammonia`. Script elements, event-handler attributes, and unsafe
link targets are not trusted or returned as executable markup.

Contest problem assignment CRUD is available under
`/api/contests/{contestId}/problems`. Super administrators and assigned contest
administrators may configure aliases, display order, and balloon colors.
Mutations lock the contest row and are accepted only while its status is
`DRAFT`; `FROZEN_CONFIG` and every later state return
`CONTEST_PROBLEM_CONFIG_FROZEN`. Database unique constraints are the final
authority for duplicate aliases and display positions.

Authenticated rostered teams use the same contest-problem list endpoint. Team
access is deliberately hidden with `CONTEST_NOT_FOUND` before `RUNNING` and for
contests in which their explicit `team_accounts` identity is not rostered.
`RUNNING`, `PAUSED`, `ENDED`, and `ARCHIVED` contests expose only the assigned
problem alias, display metadata, resource limits, allowed languages, and a
sanitized statement. The optional `lang` query selects a preferred statement;
the problem's default language and then a deterministic language order provide
fallback. Raw Markdown, test-data metadata, object keys, and attachment
metadata are not included in this team projection. A rostered team may
download a specific attachment through its opaque database ID only after the
contest reaches `RUNNING`; the service authorizes the parent problem again and
never returns the storage key.

Contest administrators load shared problem metadata through
`GET /api/problems?contestId={managedContestId}`. The API verifies that the
requested non-deleted contest is assigned to the actor before returning the
bounded catalog; omitting `contestId` or naming a foreign contest never widens
their access. Global catalog access and problem creation remain
super-administrator-only. Contest administrators can edit an assigned
problem's alias, display order, balloon color, metadata, multilingual
statements, attachments, and immutable test-data versions from the contest
workbench when every active contest using that problem is inside their assigned
scope. A foreign assignment or an unassigned problem is hidden as not found,
and any non-DRAFT assignment locks metadata, statements, attachments, and test
data. Problem deletion remains a super-administrator-only catalog operation.

`PUT /api/contests/{contestId}/problems/reorder` requires every assigned
problem exactly once, with unique positions from 1 through 1000. The operation
locks both the contest and its assignments, rejects incomplete or foreign ID
sets, and runs only in `DRAFT`. PostgreSQL defers the per-contest order
uniqueness constraint until transaction commit, allowing safe position swaps
without exposing or committing an intermediate duplicate. Rejected incomplete
and frozen requests leave the stored order unchanged.

## Object Storage And Attachments

Object storage is optional in local API configuration and mandatory for file
operations when enabled. The Rust adapter uses one shared AWS S3 client with an
explicit endpoint, region, static deployment credentials, path-style bucket
addressing, and request timeout. Readiness checks the configured problem bucket
without exposing endpoint, bucket, credentials, or SDK errors in the HTTP
response.

Problem attachment keys follow
`problems/{problemId}/attachments/{sha256}/{uuid}-{filename}`. The public
attachment response never includes the bucket or object key.

`GET /api/problems/{problemId}/statements` returns persisted Markdown and
sanitized HTML for every stored language to a scoped problem editor. It uses
the same all-assignment management rule as statement mutation.

`GET /api/problems/{problemId}/attachments` returns the persisted attachment
history to a scoped problem editor, ordered by creation time and ID. It uses
the same all-assignment management rule as attachment mutation and never
returns object-storage keys.

`POST /api/problems/{problemId}/attachments` accepts exactly one `kind` and one
`file` multipart field. `kind` is `SAMPLE` or `SUPPLEMENT`; empty files and
files larger than 20 MiB are rejected. Filenames are reduced to a safe basename
and media types are allow-listed. The service performs a database preflight,
writes the object, then locks and revalidates the problem before inserting its
SHA-256 metadata and audit row. A failed metadata transaction triggers a
best-effort delete of the newly written object. Assignments belonging to any
contest beyond `DRAFT` reject the change before commit.

`GET /api/problems/{problemId}/attachments/{attachmentId}` authorizes access
through the parent problem. Rostered teams can read attachments only from an
assigned contest in `RUNNING`, `PAUSED`, `ENDED`, or `ARCHIVED`; authorized
staff can read their scoped assignments. The response uses a safe media type,
`Content-Disposition: attachment`, and `X-Content-Type-Options: nosniff`.

`DELETE /api/problems/{problemId}/attachments/{attachmentId}` uses the same
all-assignment contest-admin rule as upload and is restricted to problems used
only by `DRAFT` contests. It commits metadata deletion and audit first, then
best-effort object cleanup so a failed storage request cannot leave a live
database reference to a missing object. Cleanup failures are logged as orphan
candidates.

The current 20 MiB bounded upload and download paths buffer one object in
memory. Failed cleanup is recorded in `object_storage_cleanup_tasks` and retried
with `FOR UPDATE SKIP LOCKED`, expiring leases, idempotent S3 deletes, and capped
exponential backoff. Explicit attachment deletion records that task in the same
transaction as metadata removal. True streaming transfer and full
reconciliation between database metadata and bucket contents remain to be
implemented.

## Immutable Test Data

`POST /api/problems/{problemId}/testdata` accepts one ZIP file up to 256 MiB.
It currently checks the filename, media type, and ZIP signature, then computes
SHA-256 and writes a version-scoped, UUID-suffixed object. The final database
transaction re-locks the problem, repeats all-assignment scope and lifecycle
checks, and advances the current pointer only if the prior version is still
current. A concurrent loser or rejected freeze race compensates by deleting
its unique object.

`problem_testdata_versions` retains the object key, hash, byte count, case count,
uploader, and creation time for every successful version. The administrator API
lists this immutable history at `GET /api/problems/{problemId}/testdata/versions`
and downloads a selected archive at
`GET /api/problems/{problemId}/testdata/versions/{version}`. Browser responses
identify the current version but never expose a durable object URL or key. Every
download verifies the stored bytes against the immutable SHA-256 metadata.
`POST /api/problems/{problemId}/testdata/versions/{version}/activate` moves the
compatibility pointer to an existing version under the same scope and lifecycle
locks used by upload. Its `expectedCurrentVersion` field prevents a stale browser
from replacing a concurrent upload or activation. A later upload allocates
`max(history.version) + 1`, so activating an older version cannot reuse an
immutable version number. `GET /api/problems/{problemId}/testdata` remains the
authorized current-version compatibility download; teams cannot call these
management endpoints. The Vue problem editor exposes the complete history,
per-version download, current marker, and guarded activation action.

Before object storage is touched, the API parses the ZIP central directory and
fully reads every regular entry on a blocking worker thread. It rejects unsafe
or nested paths, duplicate names, control characters, encryption, links,
special files, unsupported compression, inconsistent expanded sizes, more than
10,000 entries, entries over 256 MiB, total expansion over 1 GiB, and per-entry
compression ratios over 200. Root-level `.in` and `.out` files must form an
exact non-empty pair set compatible with the legacy P0 Worker; the derived case
count is persisted with the immutable version.

Judge-task construction can request an internal authoritative reference. It is
returned only when the `problems` compatibility pointer exactly matches the
same version, object key, and SHA-256 in `problem_testdata_versions`, preventing
an inconsistent or partially migrated pointer from being dispatched.

Test-data endpoints still use bounded in-memory transfer because the API
verifies the immutable SHA-256 before returning bytes. Attachment HTTP downloads
use the S3 stream directly. Failed upload compensation deletes enter the durable
object cleanup queue. Full bucket reconciliation and execution of the real
RustFS integration tests remain before test data is production-ready. The Worker
now performs bounded extraction using the same root-level case policy.

## Submission Creation Boundary

`POST /api/contests/{contestId}/submissions` preserves the reviewed legacy
multipart contract: one JSON `metadata` field and one `source` file. The API
accepts only C, C++, Java, and Python filename extensions and source bodies from
1 byte through 64 KiB. Team identity always comes from the authenticated
user's explicit `team_accounts` row; no client-supplied team ID is accepted.

Preflight and final locked validation require a non-deleted team, active
roster, assigned problem, `RUNNING` contest whose current time is inside its
schedule, enabled language, and an exact match between the problem's current
test-data pointer and its immutable version row. Source bytes are written under
`submissions/{contestId}/{teamId}/{uuid}.{extension}` in the configured source
bucket and SHA-256 is persisted.

The final PostgreSQL transaction takes a team advisory lock and enforces at
most 20 accepted submissions in the preceding minute. It atomically inserts
the submission, initial judgement UUID, serialized `JudgeTask` in
`submission_outbox`, TEAM-scoped status event, and audit row. Any final
validation, rate-limit, or transaction failure triggers best-effort deletion
of the unique source object.

## Judge Task RabbitMQ Dispatch

When RabbitMQ dispatch is enabled, Lapin connects with the configured AMQP or
AMQPS URL and declares the reviewed durable direct topology: `judge.tasks`,
`judge.retry`, `judge.dead`, optional `judge.rejudge`, and `judge.results`, each
with its matching exchange and routing key. The task queue dead-letters worker
rejections to the retry exchange; the retry queue waits 10 seconds and
dead-letters back to the task exchange.

The submission dispatcher claims PostgreSQL Outbox rows with `FOR UPDATE SKIP
LOCKED`, changes them to `PUBLISHING`, and assigns an instance UUID plus expiry
lease. Publication uses persistent JSON messages, the judgement UUID as both
AMQP message ID and `messageId` header, mandatory routing, and per-message
Publisher Confirm. Only a routed broker ACK changes the row to `SENT`; that
transaction also moves an unchanged `PENDING` submission to `JUDGING` and
writes its TEAM event.

Failures become `FAILED` with bounded exponential backoff. Attempts are capped;
expired `PUBLISHING` leases are reclaimable after API crashes. A crash after
broker ACK but before the database update can publish the same judgement UUID
again, so result consumption must remain idempotent. The publisher rebuilds a
disconnected channel and retries once. Readiness actively connects, declares
the topology, reports task/dead queue depth, and includes pending/failed
submission Outbox counts without exposing the AMQP URL.

The API consumes `judge.results` with bounded prefetch. `JudgeResult` has a
closed verdict set, bounded logs and metrics, immutable message UUID, worker
identity, timestamps, and per-test runs. One PostgreSQL transaction locks the
judgement, validates its submission, writes the final judgement and unique
runs, updates the submission, and enqueues the TEAM event. The delivery is
ACKed only after commit. The same message UUID is an idempotent ACK; a different
message cannot overwrite a completed judgement. Invalid or conflicting results
are rejected to `judge.dead`; transient database errors are requeued and force
a consumer-session reconnect.

The PostgreSQL result transaction is integration-tested. Live Docker validation
covers Publisher Confirm, task retry TTL, result ACK, duplicate-result
idempotency, malformed-result dead-lettering, RustFS round trips, and queue
depth readiness. Broker restart recovery is covered by the Docker fault-injection test.

## Judge Worker Acquisition and C/C++ Execution

The Rust Worker performs RabbitMQ consume/result-confirm/ACK boundaries,
RustFS source and immutable test-data acquisition, artifact size and SHA-256
verification, hash-keyed local test-data caching, safe root-level case
extraction, and guaranteed per-job cleanup. C and C++ use fixed runtime images
through Bollard rather than host process execution. A single container is
created per judgement: compilation and every sequential case run use Docker
exec in that container, then the container is force-removed. Compilation uses
a bounded 1 GiB allowance and the cgroup is reduced to the task memory limit
before case execution. Canonical outputs remain outside the container; only the
current input is staged into its work directory. The container request
enforces the reviewed local-development sandbox controls: no network, read-only
root, non-root user, dropped capabilities, `no-new-privileges`, PID, CPU,
memory, output and wall-time limits.

Ignored integration tests exercise a real fixed C++ image and the complete
RabbitMQ → RustFS → cache/hash → compile/run → confirmed JudgeResult path. Real
locked-down container tests cover C, C++, Java 21, and Python 3.12. Java uses an
explicit 2× time multiplier and Python 3×. Production rootless Podman/runsc
verification remains follow-up work. Docker cgroup statistics now populate per-run and aggregate
peak-memory fields, and cgroup CPU nanoseconds drive the reported run time and language-adjusted
CPU limit. A bounded wall-clock deadline remains as a safety limit and short-process fallback.
Workers now publish confirmed, versioned RabbitMQ heartbeats with a stable process instance ID,
capacity, active task count, P0 languages, image-version tags, and sandbox runtime. The API stores
them in PostgreSQL and exposes online/stale counts plus current capacity in `/api/health`.
`JUDGE_TASK_PREFETCH` is the Worker execution capacity: tasks run concurrently up to that bound,
and graceful shutdown drains already accepted work without consuming new deliveries.
An automated Docker fault test restarts RabbitMQ while a task is inside its handler. The original
unacknowledged delivery is requeued, the Worker reconnects, and exactly one confirmed result remains
visible for the stable judgement identifier.

The first ICPC scoreboard API slice exposes `GET /api/contests/{contestId}/scoreboard` and the
scope-protected `GET /api/admin/contests/{contestId}/scoreboard`. Both accept `groupName` and
`participationType` filters. During the configured freeze interval the public variant reconstructs
cells using only submissions before `freezeAt`; the administrator variant reads the live
PostgreSQL projection. Ordering is solved descending, penalty ascending, last solve ascending, then
team ID. STAR and PRACTICE rows remain visible but only OFFICIAL rows receive `officialRank`.
Per-problem First Blood is the earliest visible AC among OFFICIAL and STAR teams, with team ID as
the deterministic same-time tie-breaker; PRACTICE teams are excluded. Matching public and admin CSV
exports are available at the same paths with a `.csv` suffix and use the already-filtered/frozen
response, so exports cannot bypass scoreboard visibility rules.

Contest managers can persist an immutable board through
`POST /api/admin/contests/{contestId}/scoreboard/snapshots` and retrieve the latest matching
artifact through `GET /api/admin/contests/{contestId}/scoreboard/snapshots/latest`. The request
selects `PUBLIC` or `ADMIN` plus the same optional group and participation filters as the live
board. PostgreSQL assigns a monotonically increasing version within that exact selector, stores
the complete JSON payload and its SHA-256 digest, and records the creating user. Database triggers
reject both updates and deletes. A rejudge rebuilds the affected authoritative cell and row from
active judgements, while already-created snapshots remain unchanged for resolver and award use.

The live board optionally uses Redis when `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED=true`.
PostgreSQL stores a monotonic revision on each contest, and database triggers advance it for
projection, roster, contest-problem, schedule, and team-display changes. Cache keys include that
revision, the public/admin variant, freeze phase, and normalized filters. Redis failures fall back
to PostgreSQL; an outage during a result update cannot revive an old key after recovery.
Integration coverage now concurrently applies the same Judge Result and verifies exactly one
application plus one idempotent duplicate, clears Redis and rebuilds the same revision from
PostgreSQL, and pauses the Redis container to verify the configured operation timeout produces a
bounded cache miss. Same-second rank and First Blood ties use team ID as the final stable key.

Contest managers can rejudge one completed submission through
`POST /api/admin/contests/{contestId}/submissions/{submissionId}/rejudge`. The JSON body requires
`expectedJudgementId`, making concurrent operator actions optimistic and deterministic. In one
transaction the API supersedes the old judgement, terminally cancels any unsent old Outbox task,
creates a new active judgement and Judge task, resets the submission to `PENDING`, rebuilds the
affected scoreboard projection, emits TEAM and STAFF events, and records the audit entry. An old
task that escaped concurrently may still be delivered, but its result is acknowledged as
superseded and cannot overwrite the new active judgement. Archived contests and non-final active
judgements are rejected.

Submission browsing is available through `GET /api/contests/{contestId}/submissions` and
`GET /api/contests/{contestId}/submissions/{submissionId}` for the authenticated team, plus the
matching `/api/admin/contests/{contestId}/submissions` administrator paths. Lists are paginated and
support team, problem, status, and P0-language filters. Team queries always add the explicit
`team_accounts` identity as a database predicate, so submission IDs cannot enumerate another
team's metadata or source object. Details load the UTF-8 source from object storage only after
authorization and include active and superseded judgement history with ordered runs. Compile logs
and stderr tails remain plain text, have control characters removed, and are bounded before being
serialized.

Batch rejudge uses the persistent `batch_rejudge_tasks` and `batch_rejudge_items` tables through
contest-scoped preview, create, list, detail, pause, and resume endpoints under
`/api/admin/contests/{contestId}/rejudge-tasks`. Creation requires an exact preview count, the
confirmation text `REJUDGE {count}`, and an idempotency key; at most 10,000 completed active
judgements may be selected by problem, team, language, verdict, or submission time. A background
runner claims items with `FOR UPDATE SKIP LOCKED` and a 30-second lease. Pausing stops new claims;
resuming retains unfinished items. Each created judgement stores a unique batch item ID, so an API
crash after committing a rejudge but before updating progress recovers the same judgement instead
of scheduling another. Task counters are recomputed from terminal item rows in the same transaction
that records each outcome.

`GET /api/admin/contests/{contestId}/judge-queue/status` preserves the legacy
drain-status response for Resolver and end-of-contest operations. The Rust query
is contest-scoped and reports `PENDING` submissions, `JUDGING` submissions,
pending/leased Outbox rows, failed Outbox rows, and the database check time in a
single snapshot. `drained` is true only when all four counts are zero. Unlike the
legacy dispatcher, Rust counts `PUBLISHING` leases in `outboxPending`, because a
task without Publisher Confirm is not safely drained. Super administrators and
assigned contest administrators can read the status; other contest IDs remain
non-enumerable. The contest administration page displays these counts and
refreshes them after a single rejudge.

Contest managers can export submission metadata and active results as UTF-8 CSV from
`GET /api/admin/contests/{contestId}/exports/submissions.csv`, or all source files plus a manifest
from `GET /api/admin/contests/{contestId}/exports/submission-sources.zip`. Both endpoints reuse the
contest administrator scope check and record an audit entry. CSV text is quoted and neutralizes
spreadsheet formula prefixes. ZIP paths are generated only from numeric IDs and a restricted
problem alias; stored source size and SHA-256 are verified before an entry is written. The
synchronous ZIP compatibility endpoint rejects more than 10,000 files or 128 MiB. Larger exports
will use the later asynchronous export-task and expiring-object workflow instead of retaining an
unbounded archive in API memory. CSV serialization plus manifest generation and ZIP compression
run on Tokio's blocking pool, so large compatible exports do not stall asynchronous request and
realtime workers.

The `submission_export_tasks` migration is the durable foundation for the next step. It records
contest scope, requester, export kind, leased processing state, retry timing, output object, and
expiry. Successful outputs are required to carry both a bucket and key; processing leases and
expired-output indexing allow multiple API instances to share generation and cleanup safely.
The Rust service can now create and load scoped tasks, atomically claim available or expired-lease
work with `FOR UPDATE SKIP LOCKED`, complete only a worker's own lease, schedule bounded failure
messages for retry, and mark successful outputs expired. Export kinds use stable
`METADATA_CSV`/`SOURCES_ZIP` wire names.
When object storage is enabled, the API starts an export runner that claims one task at a time,
generates its artifact off the asynchronous worker threads, uploads it under an isolated
`exports/contests/{contestId}/` key, and publishes a 24-hour expiry. Failures use capped
exponential backoff. Completion is lease-owner guarded; if a worker uploads after losing its
lease, the uploaded object is deleted or persisted into the standard cleanup queue.
Contest managers use `POST /api/admin/contests/{contestId}/exports/tasks` to enqueue work,
`GET /api/admin/contests/{contestId}/exports/tasks/{taskId}` to poll it, and the matching
`/download` endpoint after success. Creation requires session plus both CSRF schemes; reads reuse
contest scope authorization. Downloads reject unfinished and expired tasks with stable
`EXPORT_TASK_NOT_READY` and `EXPORT_TASK_EXPIRED` conflicts.
Every runner iteration also claims up to 100 expired successful tasks with `FOR UPDATE SKIP
LOCKED`. The transition to `EXPIRED` and insertion of `EXPORT_EXPIRED` object-cleanup tasks occur
in one PostgreSQL statement, so a crash cannot leave an expired artifact outside the durable
cleanup workflow. Duplicate cleanup insertion is harmless through the existing bucket/key unique
constraint.
The task download path uses the object-storage streaming API. S3 `GetObject` bodies are bridged
directly into the Axum response body through bounded reader chunks instead of being collected into
one `Bytes` allocation. In-memory and test adapters retain a safe single-chunk default, while the
production S3 path keeps memory usage independent of artifact size.
Background artifacts are generated into collision-resistant files in the operating-system
temporary directory. S3 uploads those files through `ByteStream::from_path`, so the complete ZIP
or CSV is not copied into an upload `Bytes` buffer. The runner removes the temporary file after
both successful and failed uploads, and generation failures make a best-effort removal of partial
files. The synchronous compatibility endpoints continue using their existing bounded in-memory
responses.
Source ZIP tasks now process entries sequentially: one object is downloaded, size/hash verified,
compressed into the temporary ZIP on the blocking pool, and released before the next object is
read. The retained manifest state contains only generated paths and SHA-256 values. Consequently
generation memory is bounded by one submission source plus ZIP-library buffers rather than the
sum of every source in the contest.

Clarifications preserve the reviewed legacy routes for asking, listing a team's own questions,
staff listing/detail, replying, and closing. Team identity comes exclusively from `team_accounts`
and the contest roster. Questions are accepted only while a contest is `RUNNING` or `PAUSED`, use
an advisory transaction lock to enforce one question per team every five minutes, and validate the
`GENERAL`/`PROBLEM` shape against contest problem assignments. Staff access requires a Judge or
contest-administrator role plus contest scope (or super administrator). Every mutation atomically
writes audit and STAFF plus recipient-only TEAM events. `PUBLIC` describes whether a reply may be
converted into an announcement; it does not expose the original question to other teams.

Announcements support immediate and scheduled publishing, optimistic-version editing of published
rows, rescheduling, cancellation, pin/unpin, irreversible withdrawal, public listing, staff history,
and detail reads through the reviewed legacy paths plus `PATCH /api/announcements/{announcementId}`
for explicit editing. Writes require contest management scope and, except withdrawal, an open
`RUNNING`/`PAUSED` contest. A schedule must be in the future and no later than the contest end time.
The one-second background scheduler claims up to 100 due rows with `FOR UPDATE SKIP LOCKED`, making
publication exactly-once across API instances; it cancels rather than publishes when the contest is
no longer open. Schedule changes emit STAFF events, while publication and withdrawal emit PUBLIC
invalidation events containing only the ID and status. Private contest reads require an explicit
team roster or staff assignment, and non-published rows are never returned to team accounts.
Converting an answered `PUBLIC` Clarification locks the source row, inserts exactly one published
announcement, links both records, writes both audits, and emits Clarification plus public
announcement events in one PostgreSQL transaction. The Vue administrator page exposes creation,
history, rescheduling, cancellation, editing, pinning, and withdrawal from the contest detail page.

Print requests preserve the legacy create, own-list, operator-list, PDF-download, retry, cancel,
and reject routes. Team identity uses `team_accounts` plus the contest roster, and requests are
accepted only during `RUNNING`/`PAUSED`. Input is UTF-8 plain text capped at 20 KiB, rejects control
characters, uses a conservative 100-column/50-line A4 estimate capped at five pages, and is limited
to one request per ten minutes and twenty per team per contest under an advisory transaction lock.
PDF generation invokes `cupsfilter` without a shell, with a fixed generic PDF PPD and JCL disabled;
the result must be a bounded pure `%PDF-` document before it is archived in object storage. Only
then is the database task committed as `QUEUED`. Team PDF/list reads are owner-scoped, while queue
actions require PRINTER or super-administrator authority. Audit and STAFF plus recipient TEAM
events are transactional. The delivery runner submits archived PDFs through `lp`, persists the
CUPS job ID, monitors active and completed queues through `lpstat`, supports cancellation, and
advances requests through `PRINTING` to `COMPLETED` under recoverable database leases. Real printer
hardware remains a deployment acceptance requirement rather than an implementation gap.
