# Backup And Restore

PostgreSQL and RustFS are the authoritative backup targets. Redis is rebuildable;
RabbitMQ should be drained before a planned final backup.

## Create a backup

```text
scripts/backup/backup.sh [output-directory]
```

The default output is `backups/project-balloon-<UTC timestamp>`. A run is built
in a temporary directory and renamed only after all steps succeed. It contains:

```text
postgres/database.sql.gz
objects/<bucket>/...
deploy-config.tar.gz
manifest.txt
SHA256SUMS
```

The script uses the active `deploy/compose/.env.rust` without evaluating it as
shell code. PostgreSQL is dumped with `--clean --if-exists --no-owner`; every
RustFS bucket returned by the S3 API is copied. Runtime secrets are excluded from
the configuration archive.

Set `BACKUP_OBJECT_STORAGE_ENDPOINT` in `.env.rust` when RustFS is not reachable
from the host at `http://127.0.0.1:9000`.

Required tools are Docker Compose, gzip, sha256sum, tar, and AWS CLI v2.

## Restore a backup

Restore is intentionally destructive. It replaces the configured PostgreSQL
objects and mirrors every backed-up bucket with `aws s3 sync --delete`.

```text
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  scripts/backup/restore.sh backups/project-balloon-<timestamp>
```

Before modifying state, the script verifies every checksum, the backup format,
and the configured database name. It stops `monitor` and `app`, keeps `data`
running, restores PostgreSQL, then restores RustFS. It deliberately does not
restart application services automatically.

## Required backup points

- After account and team import.
- After problem/test-data freeze.
- Immediately before the official contest.
- Periodically during the contest.
- After the Judge queue drains.
- After Resolver, awards, and final exports.

## Post-restore verification

Run:

```text
scripts/deploy/start.sh app
scripts/deploy/start.sh monitor
scripts/deploy/healthcheck.sh all
```

Then verify contest lifecycle, accounts, problems and test-data hashes,
submission/judgement counts, public/admin scoreboards, Resolver snapshots,
awards, printing, balloon tasks, and cleanup/export backlogs. Preserve the
failed-state data and operator timeline separately before any live-contest
restore.
