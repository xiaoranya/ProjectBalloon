---
title: Disaster Recovery
description: Failure scenarios and response principles for the API, judge workers, RabbitMQ, Redis, PostgreSQL, RustFS, printers, screens, and live views.
---

# Disaster Recovery

This page defines failure scenarios and response principles for an official deployment.

## General Response Flow

```text
Detect incident
  -> identify affected service and impact
  -> preserve logs and current state
  -> apply the least disruptive recovery action
  -> verify health checks
  -> verify contest-specific correctness
  -> record incident notes
```

## API Failure

Symptoms:

- Gateway returns 5xx.
- Contestants cannot submit or view pages.
- Admin console unavailable.

Response:

- Check Nginx and API health.
- Restart affected API service if needed.
- Verify database, Redis, RabbitMQ, and RustFS connectivity.
- Confirm submissions during outage were not partially accepted without queue tasks.

## Judge Worker Failure

Symptoms:

- Worker online count decreases.
- Judge queue depth grows.
- Submissions remain `pending` or `judging` too long.

Response:

- Check worker host CPU, memory, disk, and sandbox logs.
- Restart failed worker service.
- Confirm RabbitMQ unacked messages return to queue or complete.
- Review `system_error` submissions.
- Add more judge hosts if available.

## RabbitMQ Failure

Symptoms:

- Judge tasks cannot be published.
- Workers cannot consume tasks.
- Queue metrics unavailable.

Response:

- Preserve RabbitMQ logs and data directory.
- Restart RabbitMQ if data is intact.
- Verify queues, bindings, and unacked messages.
- Requeue submissions in `pending` or stale `judging` state if required by admin action.

## Redis Failure Or Flush

Symptoms:

- Scoreboard cache missing.
- Rate-limit counters reset.
- SSE fanout degraded.

Response:

- Restart Redis.
- Rebuild scoreboard and first blood cache from PostgreSQL.
- Verify public and admin scoreboards.
- Continue contest if PostgreSQL state is intact.

## PostgreSQL Failure

Symptoms:

- API cannot read or write authoritative data.
- Submissions cannot be safely accepted.

Response:

- Stop API writes if database integrity is uncertain.
- Preserve PostgreSQL logs and data directory.
- Attempt normal restart first.
- If restore is required, follow [Backup and Restore](backup-restore.md).
- Verify submission and judgement consistency after recovery.

## RustFS Failure

Symptoms:

- Problem attachments unavailable.
- Test data cannot be fetched by workers.
- Submission code or logs cannot be archived.

Response:

- Check RustFS service and disk.
- Restart if safe.
- Verify bucket availability and object hashes.
- Pause judging if test data reads are failing widely.
- Restore RustFS objects if corruption is confirmed.

## Printer Failure

Symptoms:

- CUPS jobs fail or printer offline.
- Print queue grows.

Response:

- Check printer power, paper, network, and CUPS status.
- Retry failed jobs after printer recovery.
- Use manual download fallback for urgent team print requests.
- Keep audit status accurate.

## Screen Or Live Failure

Symptoms:

- Screen heartbeat stops.
- OBS browser source cannot load live page.
- Public display stale.

Response:

- Check gateway and page route.
- Refresh browser source or reconnect screen client.
- Verify token validity for live pages.
- Use static fallback page if dynamic display cannot recover quickly.

## Contest Pause Criteria

Consider contest pause or extension if:

- Submissions cannot be accepted for a meaningful period.
- Judging is unavailable long enough to affect contest fairness.
- Problem statements or clarification access is unavailable for many teams.
- Data correctness is uncertain.

Pause and extension decisions must be audited.

## See Also

- [Backup and Restore](backup-restore.md) — creating and restoring backups.
- [Operations](ops.md) — routine health checks.
