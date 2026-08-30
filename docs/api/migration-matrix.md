# API Migration Matrix

The legacy `openapi.yaml` is a compatibility baseline, not the specification
for new Rust behavior. Every operation moves through one of these states:

- `UNREVIEWED`: compatibility has not been assessed;
- `COMPATIBLE`: preserve the legacy HTTP contract;
- `REDESIGN`: intentionally change the contract and document the replacement;
- `IMPLEMENTED`: Rust code exists but verification is incomplete;
- `VERIFIED`: contract and integration tests pass;
- `DEPRECATED`: the operation will not be migrated.

## Reviewed Route Inventory

Snapshot date: 2026-08-30. Path-parameter names such as `{id}` and
`{contest_id}` are treated as equivalent.

| Inventory | Operations |
|---|---:|
| Legacy OpenAPI baseline | 147 |
| Current Axum router | 218 |
| Same method and normalized path | 108 |
| Explicit Rust redesigns of legacy operations | 39 |
| Rust-only replacement or extension operations | 110 |
| Unreviewed legacy operations | 0 |

Run `python3 scripts/check-api-compat.py --check` after changing either the
legacy baseline or `apps/api/src/lib.rs`. A count change intentionally fails
until this snapshot and the classification below are reviewed.

The 39 non-identical legacy routes are accounted for as follows:

| Legacy route family | Operations | Rust decision |
|---|---:|---|
| `/api/resolver/runs/*`, `/api/contests/{id}/resolver/*` | 7 | REDESIGN: persistent run resources live under `/api/admin/resolver-runs/*`; public recovery uses `/api/public/resolver-runs/{id}/state` |
| Contest-nested balloon item actions/detail | 5 | REDESIGN: item actions use `/api/balloons/{id}/*`; the ambiguous status mutation is split into `deliver` and `cancel`, while contest list rows provide item detail |
| Award categories, award generation/freeze/export/recipients | 12 | REDESIGN: all mutation and operator reads use the explicit `/api/admin/*` namespace; recipient generation is separated into generated and manual workflows |
| Single and batch rejudge legacy routes | 7 | REDESIGN: every operation is contest-scoped under `/api/admin/contests/{id}`; persistent tasks expose preview, pause, and resume instead of a lossy global cancel |
| Global submission detail and source routes | 2 | REDESIGN: team and administrator detail are contest-scoped and enforce different projections; source is returned only inside the authorized detail response |
| Group/admin scoreboard and legacy scoreboard export | 3 | REDESIGN: group selection is `GET /api/contests/{id}/scoreboard?groupName=...`; admin reads and CSV use `/api/admin/contests/{id}/scoreboard[.csv]` |
| Resolver current state | 1 | REDESIGN: immutable source selection and public current state are run-scoped rather than an implicit contest singleton |
| Problem overview | 1 | DEPRECATED: `GET /api/contests/{id}/problems` is the richer scoped projection and avoids a second inconsistent read model |
| Administrator health | 1 | REDESIGN: `/api/health` is the deployment readiness contract and `/livez` is process liveness |

These 39 operations are also registered machine-readably in
`renamed-routes.yaml` (legacy method + path → current Rust path), which
`check-api-compat.py` subtracts from the missing count so "already
redesigned" is not confused with "lost".

`GET /api/admin/contests/{contestId}/judge-queue/status` retains its legacy
method, path, and response fields. Rust counts `PUBLISHING` together with
`PENDING` Outbox rows because a leased-but-unconfirmed task is not drained.

The latest run of `scripts/test/docker-integration.sh` passed the complete
Docker-backed API, PostgreSQL, Redis, RustFS, RabbitMQ, and Judge sandbox
integration set. These remain ignored in the default offline test command
because they require a Docker daemon and fixed images.

## Domain Review Summary

| Domain | Outcome | Notes |
|---|---|---|
| Authentication, staff accounts, permissions | VERIFIED | Session, CSRF, password migration/reset, direct account permissions, and contest-scope management are covered |
| Contests, teams, roster, lifecycle, archive | VERIFIED | Includes clone, automatic milestones, pre-archive task checks, and database read-only protection |
| Problem bank and contest problems | VERIFIED | PostgreSQL service behavior and file-transfer paths pass the integration suite; the remaining catalog mutations are intentionally super-administrator-only |
| Submission, rejudge, Judge transport | VERIFIED | Contest-scoped projections, durable Outbox, RabbitMQ result idempotency, batch tasks, exports, and queue drain status exist |
| Scoreboard and snapshots | VERIFIED | Public/admin variants, group and participation query filters, freeze behavior, CSV, snapshots, and Redis cache exist |
| Clarification, announcements, printing, balloons | VERIFIED | Scheduled announcements and CUPS delivery are Rust extensions beyond the baseline |
| Resolver, awards, presentation, Screen, Live/OBS | REDESIGN | Feature-complete operator flows use explicit admin/run resources and public token-scoped views |
| Health and operations | REDESIGN | Readiness/liveness, cleanup backlog, workers, RabbitMQ and storage are represented in the Rust health model |

## Problem Slice

| Legacy operation | Status | Rust direction |
|---|---|---|
| `GET /api/problems` | VERIFIED | Super-admin bounded problem-bank listing and `contestId`-scoped Contest Admin catalog access are enforced; unscoped Contest Admin reads and foreign scopes are rejected |
| `POST /api/problems` | VERIFIED | Validates limits and the closed P0 language set; returns `201`; super-administrator-only create is covered by PostgreSQL integration tests |
| `GET /api/problems/{id}` | VERIFIED | Super-admin and all-assignment-scoped Contest Admin reads exist; foreign or unassigned access is hidden as not found |
| `PATCH /api/problems/{id}` | VERIFIED | Optimistic concurrency, all-assignment Contest Admin scope, and freeze checks pass PostgreSQL integration tests |
| `DELETE /api/problems/{id}` | VERIFIED | Soft deletion is blocked while the problem is assigned to a contest; unassigned deletion is covered by PostgreSQL integration tests |
| `PUT /api/problems/{id}/statements/{langCode}` | VERIFIED | Stores bounded Markdown, returns server-rendered sanitized HTML, and enforces all-assignment Contest Admin scope plus freeze checks |
| `GET /api/problems/{id}/statements` | VERIFIED | Returns persisted Markdown statements for scoped editors without exposing team-facing projections |
| `DELETE /api/problems/{id}/statements/{langCode}` | VERIFIED | Deletes a persisted language statement under the same scope and freeze rules as statement updates |
| `GET /api/problems/{id}/attachments` | VERIFIED | Rust extension lists persisted attachment metadata for scoped editors without exposing object keys |
| `GET /api/problems/{id}/testdata` | VERIFIED | Scoped staff download of the current archive returns safe headers without exposing object keys; streaming remains |
| `POST /api/problems/{id}/testdata` | VERIFIED | Bounded upload, deep ZIP safety and `.in`/`.out` pair validation, immutable versions, SHA-256, stale-write protection, freeze recheck, and compensation pass tests |
| `POST /api/problems/{id}/attachments` | VERIFIED | Bounded multipart upload, S3 object write, SHA-256 metadata, all-assignment scope, freeze recheck, and compensation pass PostgreSQL integration tests |
| `GET /api/problems/{id}/attachments/{attachmentId}` | VERIFIED | Parent-problem authorization, roster/lifecycle checks, safe response headers, and opaque storage metadata pass PostgreSQL integration tests |
| `DELETE /api/problems/{id}/attachments/{attachmentId}` | VERIFIED | DRAFT-only metadata deletion, audit, and best-effort object cleanup pass PostgreSQL integration tests; this is a Rust redesign extension |
| `GET /api/contests/{contestId}/problems` | VERIFIED | Scoped staff view and rostered-team publication view pass PostgreSQL integration tests |
| `POST /api/contests/{contestId}/problems` | VERIFIED | Enforces Contest Admin scope, DRAFT lifecycle, alias, order, and color validation; scoped assignment is covered by PostgreSQL integration tests |
| `PATCH /api/contests/{contestId}/problems/{problemId}` | VERIFIED | Locks contest lifecycle, maps deterministic uniqueness conflicts, and is exposed by the contest administration editor |
| `DELETE /api/contests/{contestId}/problems/{problemId}` | VERIFIED | Allows scoped removal only in DRAFT and rejects assignments with submissions |
| `PUT /api/contests/{contestId}/problems/reorder` | VERIFIED | Complete-set validation and deferred uniqueness provide atomic position exchange |
| `GET /api/contests/{contestId}/problems/overview` | DEPRECATED | Replaced by the richer scoped `GET /api/contests/{contestId}/problems` projection |
| `POST /api/contests/{contestId}/submissions` | VERIFIED | Explicit team identity, RUNNING time window, roster/problem/language checks, 64 KiB source upload, exact rolling rate limit, initial judgement, JudgeTask Outbox, realtime event, and compensation pass PostgreSQL tests |
| `GET /api/contests/{contestId}/submissions` | REDESIGN | Implement team-private and authorized staff projections with bounded cursor pagination |
| `GET /api/contests/{contestId}/submissions/{submissionId}` | REDESIGN | Hide other teams' source and sensitive Judge output; expose staff detail separately |

Test-data and attachment transfer now have verified bounded implementations.
Failed attachment, test-data, submission-source, and print-PDF compensation
deletes are persisted for multi-instance-safe background retry. Attachment
metadata deletion registers its cleanup task in the same database transaction,
closing the commit-to-RustFS-delete crash window. Attachment HTTP downloads now
use the S3 stream directly; test-data downloads still buffer while verifying the
immutable SHA-256. Bidirectional bucket-to-database reconciliation now persists
missing-reference findings and exposes their unresolved count operationally.
The Worker already performs bounded, policy-compatible
extraction.

Judge Task dispatch and Judge Result consumption are verified behind
`PROJECT_BALLOON_RABBITMQ_ENABLED`: durable topology declaration, stable
message IDs, mandatory Publisher Confirm, multi-instance Outbox leases,
exponential retry, expired-lease recovery, transactional result idempotency,
ACK-after-commit, dead-letter rejection, and readiness projection exist. Live
Docker validation covers confirm, task retry TTL, result ACK, duplicate replay,
and malformed-result dead-lettering. The broker-restart test exists but must be
executed as part of the production rehearsal rather than the default test suite.
