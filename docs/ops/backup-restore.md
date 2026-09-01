---
title: Backup and Restore
description: How to create backups, restore a deployment, the required backup points, and post-restore verification.
---

# Backup And Restore

PostgreSQL and RustFS are the authoritative backup targets. Redis is rebuildable;
RabbitMQ should be drained before a planned final backup.

## Create a backup

```text
sudo /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon
```

The output contains a `project-balloon-<UTC timestamp>` directory. A run is
built in a temporary directory and renamed only after all steps succeed. It
contains:

```text
postgres/database.sql.gz
objects/<bucket>/...
config/project-balloon.env.masked
RESTORE-CHECKLIST.md
deploy-config.tar.gz
manifest.txt
SHA256SUMS
```

The script loads the configured environment without evaluating it as shell
code. In a binary installation, `PROJECT_BALLOON_DATABASE_MODE=direct` uses
`DATABASE_URL` and the host `pg_dump` command, so Docker is not required.
PostgreSQL is dumped with `--clean --if-exists --no-owner`; every RustFS bucket
returned by the S3 API is copied. Runtime secrets are excluded from the
configuration archive.

`config/project-balloon.env.masked` is a sanitized copy of the deployment
environment: values of variables whose names end in `KEY`, `SECRET`,
`PASSWORD`, `TOKEN`, `URL`, or `DSN` are replaced with
`CHANGE_ME_redacted_from_backup`, so the archive can be stored off-host
without leaking live credentials while still recording every variable the
deployment sets. Rebuild the env file from this copy plus the operator secret
store during a restore. `RESTORE-CHECKLIST.md` carries the step-by-step
restore order for the archive.

Set `BACKUP_OBJECT_STORAGE_ENDPOINT` in
`/etc/project-balloon/project-balloon.env` when object storage is not reachable
from the host at `http://127.0.0.1:9000`.

Required tools are gzip, sha256sum, tar, PostgreSQL client tools (`pg_dump` and
`psql`), and AWS CLI v2. Legacy Compose mode additionally requires Docker
Compose and reads `deploy/compose/.env.rust`.

## Automating backups

The binary installer renders and enables `project-balloon-backup.timer` on
every API install (including `--role api`). The timer fires the
`project-balloon-backup.service` oneshot daily at 03:15 local time with a
15-minute jitter; `Persistent=true` catches up a missed run after downtime.
The service runs `backup.sh /var/backups/project-balloon` and then
`check-freshness.sh` as an `ExecStartPost`, so a drifted output path or a
misdated backup fails the unit instead of passing silently. Override the
output directory at install time with `--backup-dir PATH` (or the
`PROJECT_BALLOON_BACKUP_DIR` environment variable); rerun `install.sh` to
re-render the units after changing it.

Operate the schedule with:

```text
systemctl list-timers project-balloon-backup.timer
systemctl status project-balloon-backup.service   # last run outcome
```

To route service failures into your alerting, add a drop-in:

```text
mkdir -p /etc/systemd/system/project-balloon-backup.service.d
printf '[Unit]\nOnFailure=notify-admin@%%n.service\n' \
  > /etc/systemd/system/project-balloon-backup.service.d/on-failure.conf
systemctl daemon-reload
```

On hosts without the systemd timer — or when cron is the house standard — use
an equivalent crontab entry (`/etc/cron.d/project-balloon-backup`):

```cron
SHELL=/bin/bash
# Daily backup at 03:15 with jitter, then a staleness alarm at 09:00.
11 3 * * * root /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon >>/var/log/project-balloon-backup.log 2>&1
7 9 * * * root /opt/project-balloon/scripts/backup/check-freshness.sh /var/backups/project-balloon || logger -p daemon.alert -t project-balloon-backup "ProjectBalloon backup is stale"
```

Run only one of the timer and the crontab line. `check-freshness.sh` exits
non-zero when the newest backup under the directory is older than
`BACKUP_MAX_AGE_HOURS` (default 26 — one missed daily run plus slack) or when
no completed backup exists; point your monitoring at its exit code or read the
`daemon.alert` syslog line.

## Restore a backup

Restore is intentionally destructive. It replaces the configured PostgreSQL
objects and mirrors every backed-up bucket with `aws s3 sync --delete`.

```text
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  sudo -E /opt/project-balloon/scripts/backup/restore.sh \
  /var/backups/project-balloon/project-balloon-<timestamp>
```

Before modifying state, the script verifies every checksum, the backup format,
and the configured database name. Binary mode stops the API and Judge Worker,
restores PostgreSQL through `psql`, then restores RustFS. It deliberately does
not restart application services automatically. Legacy Compose mode stops
`monitor` and `app`, keeps `data` running, and restores PostgreSQL through the
PostgreSQL container.

## Required backup points

- After account and team import.
- After problem/test-data freeze.
- Immediately before the official contest.
- Periodically during the contest.
- After the Judge queue drains.
- After Resolver, awards, and final exports.

## Post-restore verification

Start the binary services and verify application health:

```text
sudo systemctl start project-balloon-api project-balloon-judge-worker
curl --fail http://127.0.0.1:8080/livez
curl --fail http://127.0.0.1:8080/api/health
```

Use the deployer's own procedures to verify PostgreSQL, Redis, RabbitMQ, object
storage, sandbox, proxy, printing, backup, and observability services. Then
verify contest lifecycle, accounts, problems and test-data hashes,
submission/judgement counts, public/admin scoreboards, Resolver snapshots,
awards, printing, balloon tasks, and cleanup/export backlogs. Preserve the
failed-state data and operator timeline separately before any live-contest
restore.

## See Also

- [Operations](ops.md) — routine on-site operations and monitoring.
- [Disaster Recovery](disaster-recovery.md) — failure response flows.
