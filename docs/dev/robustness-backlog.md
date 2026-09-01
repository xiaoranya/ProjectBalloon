---
title: Robustness Remediation Backlog
description: Record of the 2026-09 robustness audit — what was closed in the
  first remediation pass and which gaps remain open, with reasons and fix
  sketches for each open item.
---

# Robustness Remediation Backlog

This page records the outcome of the robustness audit performed on 2026-08-31
and 2026-09-01 across four areas (API, judge worker, frontend, and
deployment/CI). The first remediation pass closed 1 P0 and 14 P1 findings plus
selected P2/P3 items on branch `fix/robustness-remediation`. Every finding
that was **not** fixed is recorded here with the reason and a fix sketch, so
the gap is visible and can be scheduled deliberately instead of being
rediscovered during an incident.

## Status Legend

- **Closed** — fixed in the first remediation pass.
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

## Open Items

### API

- **[P2] Testdata upload buffers 256 MiB in RAM per request.** The multipart
  handler accumulates the whole field (`field.bytes().await`) before hashing
  and the S3 PUT, while the download path deliberately streams. Two
  concurrent admin uploads produce multi-hundred-MiB RSS spikes.
  *Why open:* touching the upload validation pipeline and hash computation is
  a self-contained refactor that deserves its own change plus storage-backed
  integration tests. *Fix sketch:* stream the field into S3 multipart upload
  (or a temp file), computing SHA-256 incrementally, enforcing the 256 MiB
  cap on the streaming reader.

- **[P2] No server-side PostgreSQL statement/idle-in-transaction timeouts.**
  A query on a black-holed connection during a failover can hang until the OS
  TCP timeout; pool `acquire_timeout` only bounds acquisition (now compounded
  by the bounded startup connect).
  *Why open:* a global `statement_timeout` risks killing legitimately long
  queries (export builds); the right values are a per-role deployment
  decision. *Fix sketch:* set `statement_timeout` and
  `idle_in_transaction_session_timeout` on the API's database role via
  startup SQL or `ALTER ROLE`, documented in `docs/ops`.

- **[P2] No TCP keepalive on database connections.** `sqlx` 0.9.0 does not
  expose `tcp_keepalive` on `PgConnectOptions` (verified against the pinned
  version), so keepalive requires a sqlx upgrade or a socket-layer
  workaround. Connection establishment is already bounded.

- **[P3] `/metrics` is unauthenticated.** Queue depths, worker capacity, and
  submission volumes are visible to anyone who can reach the port. Needs an
  ops decision (bind isolation, trusted-proxy CIDR, or token) before hardening.

- **[P3] Result-consumer transient failures requeue with no delay.** A
  degraded PostgreSQL causes immediate redelivery loops (bounded to roughly
  one attempt per 5s). Fix sketch: a TTL retry exchange for results, mirroring
  the task path.

### Judge worker

- **[P2] Topology verification is passive.** Passive declares do not compare
  dead-letter arguments or bindings; a mis-declared `judge.tasks` would turn
  retry-nacks into silent message loss. Fix sketch: non-passive declare with
  identical arguments (fails on mismatch) or a boot-time canary task.

- **[P3] I/O failures carry no path context** in messages surfaced to admins
  (e.g. compile logs show bare `os error 2`). Broad but mechanical wrap of
  `tokio::fs` call sites.

- **[P3] Fallback CPU metric is container-cumulative**, so reported
  `time_ms` inflates on timed-out/OOM runs. Verdicts are unaffected
  (flags decide). Fix sketch: snapshot the counter per exec and report the
  delta.

- **[P3] Docker timeouts are hard-coded** (bollard 10s client timeout,
  `DOCKER_API_TIMEOUT` 5s) rather than derived from worker config.

### Frontend

- **[P2] Large blob downloads have no timeout** (`timeoutMs: 0` exemption for
  testdata ZIPs and attachments). A mid-transfer stall is unbounded; transport
  failures still surface as `NETWORK_ERROR`. Fix sketch: progress UI with an
  explicit cancel, and an optional long cap.

- **[P3] Award display rotation clock drifts during SSE outages** because the
  local clock accumulates `+= 1000` per tick. Re-syncs on every event, so
  impact is bounded. Fix sketch: anchor to `performance.now()`.

- **[P3] Repeated `?contestId` query params coerce to NaN** on display routes
  (LiveView, AwardDisplayView, ScreenManageView). Malformed links produce a
  misleading "missing contestId" message. Fix sketch: normalize array params
  like ResolverDisplayView does.

- **[Accepted] Boot-probe outage may briefly land users on login** until the
  next navigation's re-probe succeeds; no dedicated "probing failed" surface
  was added because no redirect loop is possible. Revisit if users report
  confusion during network incidents.

### Deployment and CI

- **[P2] Compose services define no resource limits.** An unbounded
  PostgreSQL, RabbitMQ, or judge-worker can OOM a single rehearsal host.
  *Why open:* sensible limits depend on host sizing; set them together with
  pressure-test numbers rather than guessing. *Fix sketch:* `mem_limit`/`cpus`
  at minimum for judge-worker, postgres, and rabbitmq.

- **[P2] Backup remains fully manual.** No systemd timer/cron example ships,
  and no backup-age alert exists; protection rests on operator memory.
  *Fix sketch:* ship a `project-balloon-backup.timer` in the binary package
  plus a documented crontab alternative and an age check in ops docs.

- **[P2] Compose installer does not create the judge cache directory** with
  correct ownership; a root-owned auto-created bind mount breaks the worker's
  cache writes confusingly. Fix sketch: create/verify ownership in
  `scripts/deploy/install.sh`.

- **[P3] install.sh reports success before services are proven live**;
  poll `/livez` (and `/api/health`) with a deadline instead of a single
  `systemctl is-active`.

- **[P3] Observability stack and the compose `web` service lack healthchecks.**

- **[P3] install.sh prefix guard is narrower than its `rm -rf "$PREFIX/web"`.**
  Resolve the prefix and refuse system-adjacent locations, or move the old
  directory aside.

- **[P3] Binary-mode backup misses the deployment config skeleton** (a
  sanitized env-file copy and a restore checklist should be in the archive
  manifest).

- **[P3] CI hardening leftovers:** coverage summary is informational (no
  threshold), no frontend browser e2e, and the docs build does not run on
  docs-only pull requests.

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
