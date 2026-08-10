---
title: Quickstart
description: How to prepare hosts, install the release, bootstrap the first administrator, and run an official contest step by step.
---

# Quickstart

This guide gets a fresh ProjectBalloon deployment ready for an official
contest. It assumes the standard topology described in
[Deployment Topology](../architecture/deployment-topology.md); a single-host rehearsal can use the
`all` role instead. Detailed install, operation, and recovery procedures are in
[Installation](install.md), [Operations](ops.md), and [Disaster Recovery](disaster-recovery.md).

## 1. Prepare Hosts

Install the host prerequisites from trusted media before starting:

- systemd, GNU coreutils (`tar`, `gzip`, `sha256sum`).
- PostgreSQL, Redis, RabbitMQ, and RustFS (or another S3-compatible service).
- Nginx for the bundled frontend configuration.
- `postgresql-client` and AWS CLI v2 on the host that runs backups.
- CUPS packages and a configured printer when printing is enabled.
- Docker Engine or Podman on Judge hosts (production uses rootless Podman with
  `runsc`, see [ADR-001: production judge sandbox](../architecture/ADR-001-production-judge-sandbox.md)).

Provision the external services and create the databases, queues, object-storage
buckets, and credentials. The installer does not create them.

## 2. Install the Release

Copy the release archive and the matching Judge Runtime image archive to each
target host, extract them, and install the relevant role:

```text
# app/gateway host
sudo ./install.sh --role api --no-start

# judge host
sudo ./install.sh --role worker --skip-nginx --no-start \
  --container-group docker --judge-images ../judge-images
```

The first run creates `/etc/project-balloon/project-balloon.env` and exits.
Fill in the external service URLs and secrets (see
[Configuration Reference](configuration.md)), then run the installer again to import images and
start services:

```text
sudoedit /etc/project-balloon/project-balloon.env
sudo ./install.sh
```

## 3. Bootstrap the First Administrator

Once the API can reach PostgreSQL, bootstrap the first super administrator:

```text
sudoedit /etc/project-balloon/bootstrap-admin.env
sudo sh -c 'set -a; . /etc/project-balloon/bootstrap-admin.env; set +a; exec /opt/project-balloon/bin/bootstrap-admin'
```

Remove or rotate the bootstrap password immediately after it succeeds.

## 4. Verify Health

Check service status and health from the gateway host:

```text
sudo systemctl status project-balloon-api project-balloon-judge-worker
curl --fail http://127.0.0.1:8080/livez
curl --fail http://127.0.0.1:8080/api/health
```

`/api/health` returns `200` with `status: up` only when PostgreSQL (and Redis,
when realtime fanout is enabled) is ready. The full pre-contest health checklist
is in [Operations](ops.md).

## 5. Prepare the Contest

- Import teams and generate accounts.
- Create the contest and configure the schedule (start, freeze, end).
- Create problems, upload statements, attachments, and test data.
- Configure balloon colors and the printer.
- Run a test session and the pressure suite ([Pressure Test](pressure-test.md)).
- Take a backup, then freeze the contest configuration.

## 6. Run the Contest

- Monitor health, judge queue depth, worker count, and scoreboard freshness.
- Handle clarifications, balloons, and print requests from the workbenches.
- After the contest ends, drain the judge queue, generate the final board and
  Resolver snapshots, then run the official Resolver and awards.

## 7. Backup and Archive

Take backups at every mandatory backup point ([Backup and Restore](backup-restore.md)),
export results and submissions, and archive the contest.

## See Also

- [Installation](install.md) — package contents and prerequisites.
- [Configuration Reference](configuration.md) — environment variables.
- [Operations](ops.md) — running and monitoring the contest.
- [Backup and Restore](backup-restore.md) — mandatory backup points.
