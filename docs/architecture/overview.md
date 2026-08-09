# Architecture Overview

This document describes the first-version architecture for ProjectBalloon, an XCPC/ICPC competition platform. The source requirement document is `docs/requirements/project-balloon-requirements.md`.

## Goals

The platform must support official offline LAN contests with predictable recovery behavior.

Primary targets:

- At least 500 online teams and about 1,000 to 1,500 users.
- About 10,000 requests per minute, or 167 RPS.
- Peak submission throughput of 100 to 300 submissions per minute.
- Initial judge capacity of 30 to 60 concurrent judge slots.
- Fully offline deployment without public CDN, public image registry, public package mirror, or third-party SaaS dependency.
- Complete XCPC/ICPC workflow from pre-contest setup to post-contest resolver, awards, export, and archive.

## Architecture Style

The first version uses a modular monolith business backend plus an independent judge worker cluster.

The backend implementation uses a Rust Cargo workspace. The API is an Axum
modular monolith running on Tokio; the scheduler/outbox dispatcher is hosted in
the API process. Judge execution remains a separately deployable Rust worker.
Business modules are Rust modules grouped by feature rather than copies of
controller/service/repository classes. PostgreSQL access uses explicit SQLx
queries and transaction boundaries.

```text
Contestants / administrators / screen pages / live pages
  -> Nginx
  -> Web frontend + API backend
  -> PostgreSQL / Redis / RabbitMQ / RustFS
  -> Judge Scheduler
  -> Judge Worker cluster
  -> Sandbox runtime
```

This structure keeps business consistency simple while isolating judge execution risks and CPU load from API and database workloads.

## Component Responsibilities

| Component | Responsibility |
|---|---|
| Nginx | LAN entrypoint, static file serving, reverse proxy, request size/time limits |
| Web frontend | Contestant UI, admin console, screen pages, live pages, resolver, awards |
| API backend | Business API, authentication, RBAC, contest data, submissions, scoreboard, audit |
| Judge Scheduler | Submission task scheduling, retry coordination, worker visibility |
| Judge Worker | Compile, sandbox run, collect results/logs, report judge outcome |
| PostgreSQL | Authoritative business data and contest results |
| Redis | Hot caches, scoreboard cache, SSE fanout support, rate-limit counters |
| RabbitMQ | Judge task queue, retry queue, dead-letter queue |
| RustFS | S3-compatible object storage for problem assets, test data, submission archive, logs, exports, print PDFs |
| CUPS | Text printing delivery to LAN printers |
| Prometheus/Grafana | Metrics collection and dashboards |
| Loki/Promtail | Centralized logs |

## Business Modules

The backend should be organized by modules even if deployed as one service initially.

P0 modules:

- Identity and RBAC
- Team and account management
- Contest management
- Problem and test data management
- Submission management
- Judge scheduling integration
- ICPC/XCPC scoreboard
- Scoreboard freeze and snapshots
- Resolver snapshots and events
- Clarification and announcements
- Print requests
- Balloon tasks
- Award rules and recipients
- Screen control
- Live broadcast pages and tokens
- Audit logs
- Health checks and exports

## Request And Event Flow

Normal contestant submission flow:

```text
Submit source code
  -> API validates contest status, language, size, and rate limits
  -> API stores submission in PostgreSQL and source archive in RustFS
  -> API publishes judge task to RabbitMQ
  -> Worker consumes task and ACKs only after durable result write intent
  -> Worker compiles and runs in sandbox
  -> Worker writes logs/artifacts to RustFS
  -> Worker reports result to API or persistence layer
  -> Scoreboard, balloon, statistics, and SSE events are updated
```

Scoreboard update flow:

```text
Judgement finalized
  -> Update authoritative submission/judgement rows
  -> Recalculate affected team/problem state incrementally
  -> Update Redis scoreboard cache
  -> Persist snapshot when required
  -> Publish SSE event for UI refresh
```

## Real-Time Delivery

SSE is the default real-time mechanism.

Recommended event channels:

- Contest announcements
- Clarification notifications
- Submission status updates
- Scoreboard invalidation/update hints
- Screen commands
- Live broadcast events
- Resolver control events
- Balloon and print task updates

SSE payloads should avoid sensitive information on public, screen, and live channels. Source code, IP address, internal notes, and private clarification content must never be sent to public channels.

## Availability Principles

- PostgreSQL is the source of truth.
- Redis data is rebuildable and must not be the only copy of contest results.
- RabbitMQ queues must use ACK, retry, and dead-letter handling for judge tasks.
- Resolver must use immutable snapshots generated after contest finalization.
- Binary release packages must be fixed and reproducible.
- Operators must have one-command health check, backup, restore, start, stop, and status scripts.

## Initial Non-Goals

These are not first-version goals unless explicitly promoted:

- OI/IOI scoring.
- Interactive problems.
- Output-only problems.
- Code similarity detection.
- Public training system.
- Multi-tenant public OJ operation.
