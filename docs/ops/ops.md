# Operations

This document defines routine on-site operations for rehearsal and official contests.

## Operator Principles

- Prefer scripted operations over manual container commands.
- Preserve logs before restart or recovery.
- Do not clear Redis, RabbitMQ, database, RustFS, or volumes during an official contest unless following an approved recovery procedure.
- Record all manual changes in the contest operation log.
- Confirm the public scoreboard and admin scoreboard after any judge, cache, or data recovery action.

## Daily Commands

Role-level commands (`data`, `app`, `monitor`, or `all`):

```text
scripts/deploy/status.sh <role>
scripts/deploy/healthcheck.sh <role>
scripts/deploy/start.sh <role>
scripts/deploy/stop.sh <role>
scripts/deploy/restart.sh <role>
```

During an official contest, prefer service-specific restart only when scripts support it. Full host role restart can interrupt contest flow.

## Health Checklist

Check before contest starts:

- API health endpoint (`GET /api/health` — aggregates database, Redis, RabbitMQ, RustFS, queues, and workers; 200 up / 503 down) and process liveness at `/livez`.
- PostgreSQL read/write check.
- Redis read/write check.
- RabbitMQ queue publish/consume check.
- RustFS bucket read/write check.
- Judge worker online count.
- Judge queue length.
- Scoreboard cache freshness.
- CUPS printer status.
- Screen instance heartbeat.
- Live pages load without exposing sensitive information.
- Backup job success.

When CUPS delivery is enabled, the API health endpoint checks the configured
queue with `lpstat`. A DOWN `cups` component means new requests will fail into
the retryable `FAILED` state instead of being reported as printed. Verify the
queue directly with:

```text
docker compose --env-file deploy/compose/.env.rust \
  -f deploy/compose/rust-app.docker-compose.yml exec api \
  lpstat -h <host>:631 -p <queue-name>
```

The `objectCleanup` health component exposes pending and failed RustFS cleanup
tasks. Failed attachment, test-data, submission-source, and print-PDF
compensation is retried automatically. Defaults can be tuned with:

```text
PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS=5000
PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS=30
PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS=1000
PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE=50
```

Do not delete cleanup rows manually during a contest. A task may safely be
retried because S3-compatible object deletion is idempotent.

## Monitoring Metrics

The Rust API exposes Prometheus text format at `GET /metrics`. The endpoint is
unauthenticated for scraper compatibility and must be restricted to the
monitoring network by the reverse proxy or firewall. It currently exports
 realtime and Judge outbox backlog/failures, object cleanup backlog/failures,
 asynchronous export states, online Judge capacity/active slots, and daily
 practice submission/judging counts.

Example scrape job:

```yaml
- job_name: project-balloon-api
  metrics_path: /metrics
  static_configs:
    - targets: ["api:8080"]
```

Critical dashboard panels:

- API QPS.
- API p95 latency.
- HTTP 5xx rate.
- PostgreSQL connections, locks, replication or backup state if applicable.
- Redis memory usage and command latency.
- RabbitMQ queue depth and unacked messages.
- RustFS disk usage and request errors.
- Judge worker online count and occupied slots.
- Judge task wait time.
- Submission rate.
- Practice submissions today and practice jobs currently judging. Compare the
  daily count with `practice_platform_settings.daily_submission_limit` and
  investigate sustained judging backlog before users hit the concurrent limit.
- Scoreboard update latency.
- Host CPU, memory, load, and disk usage.
- Printer state and pending print tasks.

## Logs

Logs should be collected by Promtail and queried through Loki.

Important log streams:

- Nginx access/error logs.
- API application logs.
- Judge scheduler logs.
- Judge worker logs.
- RabbitMQ logs.
- PostgreSQL logs.
- RustFS logs.
- CUPS logs.

Logs must not include passwords, source code beyond explicit admin views, or live tokens.

## Common Operations

Rebuild scoreboard cache:

```text
Admin action: rebuild scoreboard cache for contest
Verify: public board, admin board, group boards, first blood state
```

Drain judge queue after contest:

```text
Wait until pending and judging submissions reach zero
Review system_error submissions
Run required rejudge operations
Generate final scoreboard
```

Backup and restore:

```text
scripts/backup/backup.sh                    # dump postgres + sync rustfs + tar config
scripts/backup/restore.sh <backup-run-dir>  # restore in order, then healthcheck + verify scoreboard
```

See `docs/ops/backup-restore.md` for mandatory backup points, contents, retention, and post-restore verification. Take a backup before rehearsal, after data freeze, immediately before contest start, and after the judge queue drains.

Daily practice operations:

- Review the `practice_submissions_today` and `practice_judging` Prometheus
  gauges during normal load.
- Configure limits from the admin console under `日常练习`; the source
  retention setting is enforced by the API cleanup runner.
- A source past its retention window is marked expired after the submission is
  final. Do not treat an expired source as a cleanup failure; inspect
  `objectCleanup.failed` and `object_storage_cleanup_failed` for actual retry
  problems.

Prepare resolver:

```text
Generate freeze and final snapshots
Run preview
Review expected final ranking
Freeze official resolver run
```

Prepare awards:

```text
Select final scoreboard source
Generate award recipients
Review conflicts and star-team settings
Freeze award list
Export CSV or Excel
```

## Incident Notes

For each incident record:

- Time.
- Affected service.
- Symptom.
- Operator.
- Action taken.
- Verification result.
- Follow-up needed.
