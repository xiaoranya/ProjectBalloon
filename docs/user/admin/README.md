---
title: Administrator Guide
description: How contest administrators manage contests, teams, problems, staff
  accounts and permissions, announcements, rejudge, backups, and archiving.
---

# Administrator Guide

This guide is for contest administrators. It covers contest lifecycle (including
cloning and extension), teams and accounts, problems and test data, staff
accounts and permissions, announcements, submission similarity review, rejudge,
audit logs, backups, and archiving. On-site ceremony tooling is covered
separately in the [On-Site Operations Guide](../onsite/README.md).

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

### Cloning a Contest

`SUPER_ADMIN` can clone an existing contest from its detail page:

- The clone copies the problem configuration (aliases, display order, colors).
- Team roster copying is optional.
- The clone is created in `DRAFT`; set a new schedule before starting it.
- Provide all three schedule fields (start, freeze, end) or none; a partial
  schedule is rejected.

### Extending a Contest

While a contest is `RUNNING` or `PAUSED`, you can extend its end time from the
contest detail page:

- The form pre-fills a new end time 30 minutes later than the current one.
- The new end time must be later than the current one.
- The request carries the current end time as an optimistic-concurrency
  check; if the contest changed concurrently, reload and retry.

## Teams and Accounts

- Create teams individually or use batch import (1–100 rows). Batch import
  requires an idempotency key: retrying the same key returns the same result
  instead of duplicating teams.
- Imported accounts require a password change at first login by default; the
  batch can override this per row.
- Team import generates accounts automatically. Distribute credentials through
  your own channel.

### Resetting a Team Password

From the team management area, you can reset a team's password:

- Choose whether the team must change the password at next login.
- The reset revokes the team's active sessions; the team signs in again with
  the new password.
- Use the reset when credentials are lost or a team must be re-authenticated.

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
| `CLARIFICATION_MANAGE` | Clarification workbench (`/judge`) |
| `PRINTING_MANAGE` | Printing workbench (`/printer`) |
| `BALLOON_MANAGE` | Balloon tasks (`/balloon`) |
| `RESOLVER_MANAGE` | Resolver control (`/resolver`) |
| `AWARD_MANAGE` | Awards management (`/awards`) |
| `SCREEN_MANAGE` | Screen wall management (`/screen/manage`) |
| `LIVE_MANAGE` | Live view management (`/live/manage`) |

`SUPER_ADMIN` is a user type, not a role: it can manage the problem bank,
practice platform settings, and staff accounts/permissions. Frontend route
visibility is not an authorization boundary; the API enforces permissions.

## Announcements

Manage announcements under `/admin/contests/:contestId/announcements`:

- Create an announcement and publish it immediately, or schedule it for a
  future time.
- Pin an announcement so it stands out on contestants' Clarification pages
  and live/screen views.
- A scheduled announcement can be edited or cancelled before it goes out.
- A published announcement can be edited or withdrawn; withdrawing removes it
  from contestant views.
- Announcement states: `SCHEDULED`, `PUBLISHED`, `WITHDRAWN`, `CANCELLED`.
- Published announcements use optimistic concurrency; if the announcement
  changed concurrently, reload and retry.

## Scoreboard, Snapshots, and Rejudge

- Public/admin/group/first-blood scoreboard variants exist with CSV export and
  snapshots. Rebuild the scoreboard cache from the admin console if it looks
  stale.
- Rejudge: bulk rejudge accepts 1–10,000 submission IDs with an expected
  count; review the task status afterwards.
- Judge queue status is available per contest; drain `PENDING`/`JUDGING` work
  before ending a contest. Do not delete outbox rows manually.

## Submission Similarity Review

`SUPER_ADMIN` can review cross-team submission similarity from the contest
detail page:

- Set a minimum similarity threshold (50–100%) and optionally filter by
  problem.
- Scan for candidate pairs; the table shows both submissions and their
  similarity percentage.
- Use the historical backfill action to compute similarity for older
  submissions that predate the feature.
- Similarity results are for manual review only: they never trigger automatic
  penalties or change judgement results.

## Audit Logs

`SUPER_ADMIN` can review the audit log on the admin home (`/admin`):

- Filter by actor user ID, action name, result (success/failed), and time
  range.
- Each record shows time, action, actor, target, source IP, and result.
- Results are paginated; use the filters to narrow down an investigation.

## Competition Mode (Workstation Binding)

In `competition` deployment mode, contestants log in from assigned
workstations with pairing codes instead of account passwords. Configure
workstation bindings under `/admin/competition` (`CONTEST_MANAGE`):

1. Register each workstation with its static IP address, seat number, and an
   optional label.
2. Load the bindings for the target contest.
3. Bind a workstation to a team. The workstation must be enabled and the team
   must be in the contest roster.
4. The pairing code is shown once — copy it and hand it to the seat. It is
   never shown again.
5. Repeat for every seat.

Operations:

- **Rotate pairing code**: generates a new code (shown once) and immediately
  invalidates the existing sessions of that binding. Use this when a code was
  leaked or after rehearsal.
- **Revoke**: unbinds the workstation and immediately invalidates its
  sessions; the seat must be bound again to log in.

Behavior:

- Pairing login works only while exactly one contest is active
  (`RUNNING`/`PAUSED` and within its schedule).
- A workstation session is bound to the workstation's IP; requests from a
  different IP are rejected.
- The session expires when the contest ends.
- Competition mode disables the daily features (problem bank, training,
  practice, virtual contests, and self-registration).

## Practice Platform

- `SUPER_ADMIN` can configure practice settings: daily submission limit
  (default 200), concurrent judging limit (default 3), and source retention
  days (default 365).
- Editorial management (`/admin/practice`, `SUPER_ADMIN`): pick a published
  problem and language, then edit the editorial title and Markdown body.
  Choose the unlock rule — always visible, after first attempt, or after the
  problem is solved — and publish it.

## Backups and Archiving

- Back up PostgreSQL, object storage, and environment files as described in
  [Backup and Restore](../../ops/backup-restore.md).
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
5. In competition mode, register and bind workstations, distribute pairing
   codes, and verify a pairing-code login from a test seat.

## See Also

- [On-Site Operations Guide](../onsite/README.md) — ceremony workbenches.
- [Competition Mode](../../ops/competition-mode.md) — workstation pairing
  deployment.
- [Daily Practice Guide](../contestant/practice.md) — contestant view of the
  practice platform.
- [Backup and Restore](../../ops/backup-restore.md) — backup points and
  restore verification.
- [Troubleshooting](../../ops/troubleshooting.md) — common failure checks.
