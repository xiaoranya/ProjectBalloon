---
title: Troubleshooting
description: Common symptoms with checks and recovery actions for API health, judge workers, queues, printers, screens, and backups.
---

# Troubleshooting

Common symptoms map to checks and recovery actions below.

## API Health Is DOWN

Symptoms: `GET /api/health` returns HTTP 503 with `status: down`, or a
component reports `DOWN`.

Checks:

- `curl --fail http://127.0.0.1:8080/livez` — if this fails, the API process
  itself is not serving; check `systemctl status project-balloon-api` and
  `journalctl -u project-balloon-api`.
- PostgreSQL reachability and credentials (`DATABASE_URL`).
- Redis reachability when `PROJECT_BALLOON_REALTIME_REDIS_ENABLED=true`.
- RabbitMQ reachability when `PROJECT_BALLOON_RABBITMQ_ENABLED=true`.
- Object-storage reachability when `PROJECT_BALLOON_OBJECT_STORAGE_ENABLED=true`.
- `lpstat -h <host>:631 -p <queue>` when `PROJECT_BALLOON_CUPS_ENABLED=true`.

A `cups` DOWN component means new print requests fail into the retryable
`FAILED` state instead of being reported as printed.

## Judge Worker Offline

Symptoms: worker online count drops, judge queue depth grows, submissions stay
`pending`/`judging`.

Checks:

- `systemctl status project-balloon-judge-worker` and its journal.
- Worker RabbitMQ connectivity and credentials.
- Sandbox socket permissions (`XCPC_SANDBOX_SOCKET`).
- Runtime images are imported and tags match the environment file.
- `JUDGE_CACHE_DIR` is writable by the worker user.
- Host CPU, memory, and disk.

A worker is considered online for 15 seconds after its latest heartbeat. After
restart, confirm unacked RabbitMQ deliveries are requeued or completed and
review any `system_error` submissions.

## Judge Queue Does Not Drain

Symptoms: `GET /api/admin/contests/{contestId}/judge-queue/status` reports
non-zero counts.

Checks:

- `PENDING`/`JUDGING` submissions — review stuck submissions and rejudge if
  needed.
- `outboxPending` includes `PUBLISHING` leases; a task without Publisher
  Confirm is not safely drained.
- `outboxFailed` rows — check RabbitMQ connectivity and the dead-letter queue.
- Worker online count and prefetch capacity.

Do not delete outbox rows manually; use the reviewed recovery paths.

## Login Or CSRF Failures

Symptoms: login returns 403/CSRF errors, mutations reject with CSRF mismatch.

Checks:

- The frontend calls `GET /api/auth/csrf` before login and sends the returned
  token in `X-XSRF-TOKEN` on every state-changing request.
- `XSRF-TOKEN` cookie and header match.
- `PROJECT_BALLOON_CSRF_SECRET` is a deployment-specific secret in production;
  `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` is not enabled there.
- `PROJECT_BALLOON_SECURE_COOKIES=true` and TLS termination exists in front of
  the API; browsers will not send `Secure` cookies over plain HTTP.
- A user flagged for password reset can only access the authentication flow
  until the password is changed.

## Scoreboard Stale Or Inconsistent

Symptoms: public board shows outdated results, cache misses, or divergence
between public and admin boards.

Checks:

- Freeze semantics: during the freeze interval the public board hides
  `submitted_at > freeze_at` submissions; the admin board shows the true state.
- `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` settings: Redis failures fall back
  to PostgreSQL; verify the cache TTL and timeout are sane.
- Rebuild the scoreboard cache from the admin console and compare public,
  admin, group, and first-blood views.
- Judge results applied correctly; confirm submissions/judgements counts.

## Print Failures

Symptoms: print requests stuck in `QUEUED`/`PRINTING`, CUPS jobs fail, printer
offline.

Checks:

- Printer power, paper, network, and CUPS status (`lpstat -h <host>:631 -p <queue>`).
- `PROJECT_BALLOON_CUPS_ENABLED` and `PROJECT_BALLOON_CUPS_PRINTER` match the
  configured queue.
- Retry failed jobs after printer recovery, or use the manual download
  fallback for urgent requests.
- Audit status remains accurate.

## Screen Or Live Page Problems

Symptoms: screen heartbeat stops, OBS browser source cannot load the live
page, display is stale.

Checks:

- Gateway and page route availability.
- Token validity for live pages (rotate rehearsal-shared tokens before the
  official contest).
- Refresh the browser source or reconnect the screen client.
- Use the static fallback page if the dynamic display cannot recover quickly.

## Backup Or Restore Issues

Symptoms: backup fails, restore verification fails.

Checks:

- `BACKUP_OBJECT_STORAGE_ENDPOINT` is reachable from the backup host.
- Required tools exist (`pg_dump`, `psql`, `sha256sum`, AWS CLI v2).
- Restore requires `PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA`.
- After restore, follow the post-restore verification in
  [Backup and Restore](backup-restore.md) before a live contest.

## Object Cleanup Backlog

Symptoms: `objectCleanup` health component shows pending/failed tasks,
`object_storage_cleanup_failed` grows.

Checks:

- Failed attachment, test-data, submission-source, and print-PDF compensation
  is retried automatically with backoff.
- Do not delete cleanup rows manually during a contest; S3-compatible deletion
  is idempotent and tasks may be retried safely.
- Inspect the cleanup runner logs for storage connectivity errors.

## Metrics Missing

Symptoms: `GET /metrics` unreachable or empty.

Checks:

- The endpoint is unauthenticated; confirm the reverse proxy or firewall
  restricts it to the monitoring network.
- Prometheus scrape target matches `api:8080` (or the configured bind).
- Required dashboard panels and label names match the exported metrics.

## General Incident Record

For every incident, record: time, affected service, symptom, operator, action
taken, verification result, and follow-up needed. Preserve logs and current
state before restart or recovery.

## See Also

- [Disaster Recovery](disaster-recovery.md) — failure response flows.
- [Backup and Restore](backup-restore.md) — restore verification.
