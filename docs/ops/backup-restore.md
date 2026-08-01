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

The script loads the configured environment without evaluating it as shell
code. In a binary installation, `PROJECT_BALLOON_DATABASE_MODE=direct` uses
`DATABASE_URL` and the host `pg_dump` command, so Docker is not required.
PostgreSQL is dumped with `--clean --if-exists --no-owner`; every RustFS bucket
returned by the S3 API is copied. Runtime secrets are excluded from the
configuration archive.

Set `BACKUP_OBJECT_STORAGE_ENDPOINT` in `.env.rust` when RustFS is not reachable
from the host at `http://127.0.0.1:9000`.

Required tools are gzip, sha256sum, tar, PostgreSQL client tools (`pg_dump` and
`psql`), and AWS CLI v2. Legacy Compose mode additionally requires Docker
Compose and reads `deploy/compose/.env.rust`.

## Restore a backup

Restore is intentionally destructive. It replaces the configured PostgreSQL
objects and mirrors every backed-up bucket with `aws s3 sync --delete`.

```text
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  scripts/backup/restore.sh backups/project-balloon-<timestamp>
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
