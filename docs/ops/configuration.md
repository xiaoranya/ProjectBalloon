---
title: Configuration Reference
description: >-
  Environment variables for the API and Judge Worker in binary deployments:
  database, object storage, realtime, scoreboard cache, printing, judge worker,
  and sandbox.
---

# Configuration Reference

This document is the configuration reference for binary deployments. The
installer creates `/etc/project-balloon/project-balloon.env`; the API and
Judge Worker read it through their systemd units. Local development uses the
root `.env` file copied from `.env.example` (see [Local Development](../dev/local-development.md)).

## Configuration Rules

- Commit only `.env.example` and non-secret templates. Real `.env` files are
  deployment-specific and must never be committed.
- Replace every development default before an official contest.
- Use separate credentials for PostgreSQL, Redis, RabbitMQ, RustFS, and the
  application.
- Do not log raw session tokens, CSRF tokens, passwords, or live tokens.
- After changing the environment file, restart the affected service:

```text
sudo systemctl restart project-balloon-api project-balloon-judge-worker
```

## API And Deployment

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_API_BIND` | `127.0.0.1:8080` | API listening socket; the reverse proxy connects here |
| `PROJECT_BALLOON_DEPLOYMENT_MODE` | `standard` | `competition` enables non-overlapping schedules and IP-bound workstation pairing and disables daily features |
| `PROJECT_BALLOON_TRUSTED_PROXY_CIDRS` | `127.0.0.1/32,::1/128` | CIDRs whose `X-Forwarded-*` headers the API trusts |
| `RUST_LOG` | `info` | Structured log level for API and Worker |
| `XCPC_API_PROXY_TARGET` | `http://127.0.0.1:18080` | Frontend dev-server proxy target (Vite only) |

## Database

| Variable | Default | Purpose |
|---|---|---|
| `DATABASE_URL` | unset | PostgreSQL connection URL with deployment credentials; there is no credentialed default |
| `PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL pool bound |
| `PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | Pool backpressure timeout |
| `PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS` | `1000` | Readiness probe timeout |
| `PROJECT_BALLOON_RUN_MIGRATIONS` | `true` | Run embedded SQLx migrations at startup; disable only when migrations are managed by a separate reviewed step |
| `PROJECT_BALLOON_BOOTSTRAP_ADMIN_*` | dev values | First administrator credentials used by `bootstrap-admin` |

## Object Storage (RustFS)

Object storage is mandatory for file operations (attachments, test data,
submission sources, exports, print PDFs).

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_OBJECT_STORAGE_ENABLED` | `false` | Enable the S3-compatible adapter |
| `PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT` | `http://127.0.0.1:9000` | RustFS/S3 endpoint |
| `PROJECT_BALLOON_OBJECT_STORAGE_REGION` | `us-east-1` | S3 region |
| `PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY` / `SECRET_KEY` | dev values | Static deployment credentials |
| `PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET` | `xcpc-problems` | Problem attachments and test data |
| `PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET` | `xcpc-sources` | Submission source files |
| `PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE` | `true` | Path-style bucket addressing for RustFS |
| `PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS` | `5000` | Per-request S3 timeout |

The API creates its configured buckets on startup. Bucket names are deployment
configuration and are never returned through public APIs.

## RabbitMQ And Judge Dispatch

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_RABBITMQ_ENABLED` | `false` | Enable durable judge-task dispatch through RabbitMQ |
| `PROJECT_BALLOON_RABBITMQ_URL` | unset | AMQP/AMQPS URL with credentials |
| `PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS` | `5000` | Broker operation timeout |
| `PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS` | `500` | Outbox polling interval for the submission dispatcher |
| `PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS` | `30` | Outbox claim lease |
| `PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS` | `1000` | Initial retry backoff |
| `PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE` | `50` | Maximum rows claimed per poll |
| `PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS` | `8` | Attempts before operator intervention |
| `PROJECT_BALLOON_JUDGE_RESULT_PREFETCH` | `32` | Result consumer prefetch |
| `PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS` | `1000` | Consumer reconnect delay |

## Browser Sessions And CSRF

Production requires TLS termination in front of the API, `Secure` cookies, and
an independently generated CSRF secret.

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_SESSION_TTL_SECONDS` | `43200` | Browser session lifetime |
| `PROJECT_BALLOON_SECURE_COOKIES` | `false` | Add the `Secure` cookie attribute; must be `true` in production |
| `PROJECT_BALLOON_CSRF_SECRET` | dev-only value | HMAC secret for CSRF tokens; the public dev value is rejected unless `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET=true`, and is refused outright with secure cookies |
| `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` | `false` | Local-development escape hatch only |

## Realtime Outbox And SSE

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED` | `true` | Claim and publish durable outbox rows |
| `PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY` | `1024` | Per-process SSE broadcast buffer |
| `PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS` | `250` | Outbox polling interval |
| `PROJECT_BALLOON_REALTIME_LEASE_SECONDS` | `30` | Recovery lease for abandoned claims |
| `PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS` | `1000` | Initial failed-delivery backoff |
| `PROJECT_BALLOON_REALTIME_BATCH_SIZE` | `100` | Maximum rows claimed per poll |
| `PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS` | `8` | Delivery attempts before operator intervention |
| `PROJECT_BALLOON_REALTIME_REDIS_ENABLED` | `false` | Publish/subscribe SSE fanout through Redis; enable on every API replica in multi-instance deployments |
| `REDIS_URL` | unset | Redis connection URL with ACL credentials |
| `PROJECT_BALLOON_REALTIME_REDIS_CHANNEL` | `xcpc:realtime:events` | Version-1 realtime Pub/Sub channel |
| `PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS` | `1000` | Subscriber reconnect delay (exponential, capped at 30 s) |

## Scoreboard Cache

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` | `false` | Cache rendered scoreboard variants in Redis while PostgreSQL stays authoritative |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS` | `30` | Expiry for revision-scoped cache entries |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS` | `200` | Max Redis wait before PostgreSQL fallback |

## Object Cleanup

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS` | `5000` | Cleanup runner polling interval |
| `PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS` | `30` | Cleanup task lease |
| `PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS` | `1000` | Initial retry backoff |
| `PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE` | `50` | Tasks claimed per iteration |

## CUPS Printing

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_CUPS_ENABLED` | `false` | Enable PDF generation and CUPS delivery |
| `PROJECT_BALLOON_CUPS_PRINTER` | `xcpc` | CUPS queue name checked by health and used by `lp` |
| `PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS` | `5000` | Timeout for CUPS commands |

## Judge Worker

| Variable | Default | Purpose |
|---|---|---|
| `WORKER_ID` | `worker-local` | Stable worker instance identity for heartbeats |
| `JUDGE_CACHE_DIR` | `/var/cache/judge` | Local test-data cache directory |
| `JUDGE_TASK_QUEUE` | `judge.tasks` | Task queue to consume |
| `JUDGE_TASK_PREFETCH` | `1` | Parallel execution capacity; graceful shutdown drains in-flight work |
| `JUDGE_RECONNECT_MILLISECONDS` | `1000` | RabbitMQ reconnect delay |
| `JUDGE_HEALTH_PORT` | `9101` | Loopback-only port for `GET /livez` (always 200) and `GET /readyz` (200 once a consume session is up and no recent session failure); the compose healthcheck probes `readyz` |
| `JUDGE_HEALTH_SESSION_ERROR_WINDOW_SECONDS` | `60` | How long a recent broker-session failure keeps `readyz` at 503 |
| `JUDGE_HEARTBEAT_INTERVAL_SECONDS` | `5` | Heartbeat publication interval |
| `JUDGE_REQUEST_TIMEOUT_MILLISECONDS` | `10000` | Storage/sandbox request timeout |
| `JUDGE_MAX_ARTIFACT_BYTES` | `314572800` | Maximum artifact size accepted per task |
| `PROJECT_BALLOON_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS` | `60` | How often the API re-enqueues judge tasks whose submission has been stuck in `JUDGING` for over 30 minutes with a SENT outbox row (self-healing complement to the `SubmissionsStuckJudging` alert; requeued tasks never exceed the dispatcher's max attempts) |

## Sandbox

| Variable | Default | Purpose |
|---|---|---|
| `XCPC_SANDBOX_SOCKET` | `/var/run/docker.sock` | Docker or rootless Podman socket path |
| `XCPC_SANDBOX_RUNTIME` | unset | `runsc` in production (ADR-001); leave empty only for the documented local Docker profile |
| `XCPC_SANDBOX_USER` | `1000:1000` | Non-root UID/GID inside sandbox containers; production uses `10001:10001` |
| `JUDGE_C_IMAGE` | `judge-runtime-c:12.2.0` | C runtime image tag |
| `JUDGE_CPP_IMAGE` | `judge-runtime-cpp:12.2.0` | C++ runtime image tag |
| `JUDGE_JAVA_IMAGE` | `judge-runtime-java:21` | Java 21 runtime image tag |
| `JUDGE_PYTHON_IMAGE` | `judge-runtime-python:3.12.13` | Python 3.12 runtime image tag |

Runtime image tags must be fixed; `latest` is never allowed. Production must
use the rootless Podman socket, uid/gid `10001:10001`, and `runsc`.

### Time-Limit Semantics (CPU Time vs. Wall Clock)

Sandboxed runs are penalized on **CPU time**, not wall-clock time
(`apps/judge-worker/src/sandbox/runner.rs`):

- The **charged time** for each test case is the CPU time reported by GNU
  `time` (`%U` user + `%S` system, parsed from the
  `__PROJECT_BALLOON_GNU_TIME__` marker). It is what `total_time_ms` and the
  time-limit comparison use.
- The **hard kill** is a wall-clock cap of `3 ×` the effective time limit
  (base limit × language multiplier), with a floor of 1 second. Exceeding the
  wall cap kills the container and the run is judged `TIME_LIMIT_EXCEEDED`.

Consequence (inherent tradeoff of the CPU-time model): a program that sleeps
until ~2.9× the effective time limit and then exits immediately consumes
almost no CPU time and will be judged `ACCEPTED` despite using far more wall
time than the limit. The 3× wall cap exists to bound resource usage, not to
enforce the time limit itself.

## Backup

| Variable | Default | Purpose |
|---|---|---|
| `PROJECT_BALLOON_DATABASE_MODE` | `direct` | `direct` uses host PostgreSQL client tools; `compose` is for legacy single-host deployments |
| `BACKUP_OBJECT_STORAGE_ENDPOINT` | `http://127.0.0.1:9000` | Override when RustFS is not reachable from the backup host at the default endpoint |

## See Also

- [Installation](install.md) — where the environment file is created.
- [Troubleshooting](troubleshooting.md) — checks that reference these variables.
- [Local Development](../dev/local-development.md) — development `.env` setup.
