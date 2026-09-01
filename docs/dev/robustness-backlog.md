---
title: Robustness Remediation Backlog
description: Record of the 2026-09 robustness audit — what was closed in the
  three remediation passes and which gaps remain open, with reasons and fix
  sketches for each open item.
---

# Robustness Remediation Backlog

This page records the outcome of the robustness audit performed on 2026-08-31
and 2026-09-01 across four areas (API, judge worker, frontend, and
deployment/CI). The findings were remediated in three passes: the first closed
1 P0 and 14 P1 findings plus selected P2/P3 items on branch
`fix/robustness-remediation`; the second closed two judge-worker P2 findings;
and the third closed the remaining API, judge-worker, frontend, and
deployment/CI findings. Every finding that is **not** fixed is recorded here
with the reason and a fix sketch, so the gap is visible and can be scheduled
deliberately instead of being rediscovered during an incident.

## Status Legend

- **Closed** — fixed in one of the remediation passes.
- **Deferred** — postponed by an explicit recorded decision (with the date);
  revisit when the blocking dependency changes.
- **Open** — verified against the code, deliberately deferred; the reason and
  a fix sketch are listed. Revisit each item before the first beta release
  and before any on-site contest.

## Closed in the First Remediation Pass

| Area | Severity | Item |
| --- | --- | --- |
| Judge worker | P0 | Zip-bomb via declared-size lying output-only archives, amplified into a poison-message crash loop (`sandbox/archive.rs` actual-bytes budget) |
| Judge worker | P1 | Output-only invalid archives now produce a contestant WrongAnswer instead of SystemError |
| Judge worker | P1 | Per-task wall-clock deadline derived from the task contract; expiry routes through the existing retry budget |
| Judge worker | P1 | Orphan container/job-dir GC (startup + periodic sweep), 409 create-conflict retry, drain bound derived from in-flight deadlines |
| Judge worker | P1 | Per-delivery error isolation; only protocol failures end the AMQP session |
| Judge worker | P2 | `memory_swap` update failure on swap-less hosts is non-fatal (memory-only retry) |
| Judge worker | P3 | A failed post-success cleanup no longer discards a completed judgement |
| API | P1 | SSE streams terminate on shutdown; graceful shutdown can complete while clients are connected |
| API | P1 | Object-storage upload timeout split from the 5s request timeout (`PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS`, default 300000) |
| API | P2 | All RabbitMQ channel awaits bounded; publish no longer holds the channel mutex across reconnection; consumer setup loops take the reconnect path |
| API | P2 | `BackgroundRunners::shutdown` joined under a 30s deadline; stragglers aborted and named; pool close always runs |
| API | P2 | Database connections built from `PgConnectOptions` with a bounded startup connect (10s) |
| API | P3 | `OptionalAuthContext` degrades to anonymous on stale/invalid cookies instead of 401 |
| Frontend | P1 | API client request timeout (30s), `NETWORK_ERROR` mapping for browser-level failures, truncated non-JSON error bodies |
| Frontend | P1 | Transparent CSRF recovery: `CSRF_INVALID` clears, refetches, and replays once |
| Frontend | P1 | CodeEditor emits immediately; stale-code submit window removed |
| Frontend | P1 | Session init no longer caches a failed probe as initialized; every navigation re-probes |
| Frontend | P1 | Screen client re-registers with capped backoff after boot failure or token invalidation |
| Frontend | P2 | LiveView stale-response generation guard; first load no longer blocks the poller |
| Frontend | P2 | Scoreboard shows a visible stale indicator when silent polling fails |
| Frontend | P2 | Logout failure no longer leaves a half-cleared UI (all role layouts) |
| Deployment | P1 | Release workflow gates `publish` on all required CI check-runs of the tagged SHA (fail-closed) |
| Deployment | P1 | npm audit for production dependencies: PR-time step plus nightly scheduled job |
| Deployment | P1 | Compose passes through `PROJECT_BALLOON_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS`, `JUDGE_HEALTH_PORT`, `JUDGE_HEALTH_SESSION_ERROR_WINDOW_SECONDS` |
| Deployment | P2 | `.env.example` gains the 5 vars the code reads, plus a judge-worker validation warning comment |
| Deployment | P2 | Toolchain pinned to 1.94.1 in `rust-toolchain.toml`; CI, release, and docker-integration workflows derive their pins from it (closed in the 0.1.0-alpha.4 release-prep pass) |

## Closed in the Second Remediation Pass (2026-09-01)

| Area | Severity | Item |
| --- | --- | --- |
| Judge worker | P2 | Interactive GNU-time report is emitted directly on the exec stderr stream — a channel the contestant process holds no descriptor for; contestant-writable diagnostics (`program.err`, `interactor.err`) are read back by a separate exec and appended only after the marker has been parsed, so a forged marker can no longer be the last one. Regression-tested with a docker-integration spoof attempt |
| Judge worker | P2 | Testdata cache is bounded by a size-capped LRU (`JUDGE_TESTDATA_CACHE_MAX_BYTES`, default 8 GiB, `0` disables): cache hits refresh their mtime, inserts evict oldest-mtime entries first with eviction logging, and the entry just stored is never evicted by its own insertion |

## Closed in the Third Remediation Pass (2026-09-01)

| Area | Severity | Item |
| --- | --- | --- |
| API | P2 | Testdata upload streams into a `0600` temp file with incremental SHA-256, a streaming 256 MiB cap, and a streaming S3 PUT — concurrent admin uploads no longer buffer hundreds of MiB in RSS |
| API | P3 | `/metrics` optionally requires a bearer token (`PROJECT_BALLOON_METRICS_TOKEN`, constant-time comparison); compose passthrough, env templates, and the configuration reference document it |
| API | P3 | Result-consumer transient failures are requeued after a TTL delay through `judge.results.retry` (bounded to 20 attempts via `x-retry-count`, then routed to `judge.dead`), ending the immediate redelivery loop during PostgreSQL degradation |
| Judge worker | P2 | Topology verification is a non-passive full-argument declare (dead-letter arguments, TTL, bindings — kept identical to the API's `topology::declare`); a mismatch fails the run with an operator hint instead of silently dropping retry-nacks |
| Judge worker | P3 | Filesystem errors carry path context via `with_path_context`, so admin-facing compile/run logs no longer show a bare `os error 2` |
| Judge worker | P3 | Fallback CPU metric is a per-exec one-shot stats snapshot delta instead of a container-cumulative counter, so timed-out/OOM runs no longer inflate `time_ms` |
| Judge worker | P3 | Docker timeouts derive from config (`JUDGE_DOCKER_CONNECT_TIMEOUT_SECONDS`, `JUDGE_DOCKER_API_TIMEOUT_MILLISECONDS`) instead of hard-coded values |
| Frontend | P2 | Large blob downloads carry a 10-minute cap with streaming progress and an explicit cancel button (TestdataTab), replacing the unbounded `timeoutMs: 0` exemption |
| Frontend | P3 | Award rotation clock anchors to `performance.now()` and no longer drifts during SSE outages |
| Frontend | P3 | Repeated `?contestId` query params normalize through `numericQueryId` across the display views instead of coercing to NaN |
| Deployment | P2 | Compose services declare `mem_limit`/`cpus` (postgres 1g, rabbitmq 1g, judge-worker 2g by default; every limit is env-overridable) across the data, app, and observability stacks |
| Deployment | P2 | Backups are automated: the installer renders and enables `project-balloon-backup.service`/`.timer` (daily 03:15 + jitter, catch-up on missed runs, freshness assertion as `ExecStartPost`), `scripts/backup/check-freshness.sh` alarms past 26 h, and ops docs carry a crontab alternative |
| Deployment | P2 | The compose installer creates the judge cache directory and asserts its ownership matches `XCPC_SANDBOX_USER` (or fails with the exact `chown` command), so a root-owned bind mount can no longer break worker cache writes |
| Deployment | P3 | `install.sh` gates "installation complete" on deadline-bounded `curl` polls of `/livez` (120 s default) for the API and the Judge Worker instead of a single `systemctl is-active` |
| Deployment | P3 | All six observability services and the compose `web` service carry healthchecks (endpoints and probe tools verified against the shipped images) |
| Deployment | P3 | The installer prefix guard resolves through `readlink -m` and refuses the full set of system roots; the previous frontend bundle is moved to a timestamped `web.old-*` directory instead of `rm -rf` |
| Deployment | P3 | Backups include a sanitized env snapshot (credential-bearing variables replaced with `CHANGE_ME_redacted_from_backup`) and a restore checklist inside the archive manifest |
| CI | P3 | The coverage summary enforces an llvm-cov line threshold instead of being informational; the docs workflow also builds docs-only pull requests (Pages deploy stays main-only) |

## Open Items

### API

- **[Deferred by user decision (2026-09-01)] No server-side PostgreSQL
  statement/idle-in-transaction timeouts.** A query on a black-holed
  connection during a failover can hang until the OS TCP timeout; pool
  `acquire_timeout` only bounds acquisition (now compounded by the bounded
  startup connect). The user chose to postpone this: a global
  `statement_timeout` risks killing legitimately long queries (export
  builds); the right values are a per-role deployment decision. When
  revisited, set `statement_timeout` and
  `idle_in_transaction_session_timeout` on the API's database role via
  startup SQL or `ALTER ROLE`, documented in `docs/ops`.

- **[Deferred by user decision (2026-09-01)] No TCP keepalive on database
  connections.** `sqlx` 0.9.0 does not expose `tcp_keepalive` on
  `PgConnectOptions` (verified against the pinned version), so keepalive
  requires a sqlx upgrade or a socket-layer workaround. The user chose to
  postpone this until the dependency surface changes; connection
  establishment is already bounded.

### Frontend

- **[Accepted] Boot-probe outage may briefly land users on login** until the
  next navigation's re-probe succeeds; no dedicated "probing failed" surface
  was added because no redirect loop is possible. Revisit if users report
  confusion during network incidents.

### Deployment and CI

- **[P3] No frontend browser e2e in CI.** The frontend is covered by unit
  tests, lint/format gates, and the OpenAPI drift check, but no headless
  browser run exercises the real UI end to end. Fix sketch: a Playwright job
  against a locally built API + web pair, kept out of per-PR CI until the
  runtime cost is measured (candidate for the nightly schedule).

## Verified Solid (no action needed)

The audit also confirmed the following are sound and should not need
re-auditing unless the design changes: the outbox dispatch pattern
(`SKIP LOCKED`, lease ownership, attempt caps, backoff), submit-path
transactional consistency with S3 compensation, judge-result idempotent
application, SQL injection surface (all interpolation is compile-time
constant or whitelisted), login rate limiting and constant-time comparisons,
sandbox hardening (no network, cap-drop-all, noexec, `O_NOFOLLOW` output
reads, path-traversal-safe zip entry names), frontend XSS surface (all
`v-html` content is server-sanitized via ammonia), timer cleanup and route
guards, migration transactionality with the frozen-checksum gate actually
enforced by CI, and secrets hygiene (no real credentials committed).
