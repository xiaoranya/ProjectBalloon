# Administrator Guide

This guide covers contest administration: lifecycle, teams and accounts,
problems and test data, staff accounts and permissions, rejudge, backups, and
archiving. The on-site ceremony tooling is covered separately in
[`../onsite/README.md`](../onsite/README.md).

## Contest Lifecycle

A contest moves through a fixed state machine:

```text
DRAFT -> FROZEN_CONFIG -> RUNNING <-> PAUSED -> ENDED -> ARCHIVED
```

- `DRAFT`: build problems, teams, and schedule; nothing is visible to teams.
- `FROZEN_CONFIG`: configuration is locked; start the contest to begin.
- `RUNNING`: teams submit; pause and resume are allowed.
- `ENDED`: judging drains and the final scoreboard is generated.
- `ARCHIVED`: contest data is retained but no longer active.

The schedule includes start time, freeze time, and end time. The freeze time
freezes the public scoreboard while judging continues in the background.

## Teams and Accounts

- Create teams individually or use batch import (1–100 rows). Batch import
  requires an idempotency key: retrying the same key returns the same result
  instead of duplicating teams.
- Imported accounts require a password change at first login by default; the
  batch can override this per row.
- Team import generates accounts automatically. Distribute credentials through
  your own channel.

## Problems and Test Data

- Create problems with a slug, title, statement (multiple languages), judge
  mode (`STANDARD` or `INTERACTIVE`), limits, and language availability.
- Statements are stored as bounded Markdown and rendered as sanitized HTML.
- Attachments and test data are versioned: a new upload creates a new version,
  and edits are rejected if the problem changed concurrently (optimistic
  concurrency).
- Test data upload is a ZIP archive with these rules:
  - 1 to 10,000 entries, up to 256 MiB.
  - Test cases must be paired `.in`/`.out` files at the archive root (no
    subdirectories).
  - Path traversal and duplicate names are rejected.
- After the freeze time, problem edits and test-data uploads are rejected.

## Staff Accounts and Permissions

Staff authorization uses direct permissions (not roles). The permission codes
are:

| Permission | What it grants |
| --- | --- |
| `CONTEST_MANAGE` | Create/edit contests, teams, problems, announcements, rejudge |
| `CLARIFICATION_MANAGE` | Judge/Clarification workbench (`/judge`) |
| `PRINTING_MANAGE` | Printing workbench (`/printer`) |
| `BALLOON_MANAGE` | Balloon tasks (`/balloon`) |
| `RESOLVER_MANAGE` | Resolver control (`/resolver`) |
| `AWARD_MANAGE` | Awards management (`/awards`) |
| `SCREEN_MANAGE` | Screen wall management (`/screen/manage`) |
| `LIVE_MANAGE` | Live view management (`/live/manage`) |

`SUPER_ADMIN` is a user type, not a role: it can manage the problem bank,
practice platform settings, and staff accounts/permissions. Frontend route
visibility is not an authorization boundary; the API enforces permissions.

## Scoreboard, Snapshots, and Rejudge

- Public/admin/group/first-blood scoreboard variants exist with CSV export and
  snapshots. Rebuild the scoreboard cache from the admin console if it looks
  stale.
- Rejudge: bulk rejudge accepts 1–10,000 submission IDs with an expected
  count; review the task status afterwards.
- Judge queue status is available per contest; drain `PENDING`/`JUDGING` work
  before ending a contest. Do not delete outbox rows manually.

## Practice Platform

- `SUPER_ADMIN` can configure practice settings: daily submission limit
  (default 200), concurrent judging limit (default 3), and source retention
  days (default 365).

## Backups and Archiving

- Back up PostgreSQL, object storage, and environment files as documented in
  [`../../ops/backup-restore.md`](../../ops/backup-restore.md).
- Restore requires the explicit acknowledgment
  `PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA` and
  post-restore verification before a live contest.
- Archiving a contest closes the active lifecycle; keep backups of the archive
  per your retention policy.

## Before a Live Contest

1. Import teams and verify credentials.
2. Upload test data and validate a sample submission.
3. Freeze configuration, back up, and only then start the contest.
4. Verify freeze time, balloon colors, printer, and scoreboard cache.
