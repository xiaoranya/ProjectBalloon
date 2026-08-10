---
title: On-Site Operations Guide
description: How on-site staff use the screen wall, live broadcast, balloons,
  printing workbench, Resolver ceremony, and awards presentation.
---

# On-Site Operations Guide

This guide is for staff working an on-site (ICPC/XCPC-style) event. It covers
the tools used during the event: screen walls, live broadcast, balloons,
printing, the Resolver ceremony, and awards. The workbenches require staff
accounts with the matching permissions; see the
[Administrator Guide](../admin/README.md) for how permissions are assigned.

## Staff Workbenches

| Workbench | Route | Permission |
| --- | --- | --- |
| Clarifications | `/judge` | `CLARIFICATION_MANAGE` |
| Printing | `/printer` | `PRINTING_MANAGE` |
| Balloons | `/balloon` | `BALLOON_MANAGE` |
| Resolver | `/resolver` | `RESOLVER_MANAGE` |
| Awards | `/awards` | `AWARD_MANAGE` |
| Screens | `/screen/manage` | `SCREEN_MANAGE` |
| Live | `/live/manage` | `LIVE_MANAGE` |

## Clarification Workbench (`/judge`)

- Review new Clarifications, reply privately, convert a question into a public
  announcement, or close it.
- Announcements are pushed to contestants' Clarifications page and shown on
  live/screen views as configured.

## Screen Wall (`/screen`)

- `/screen/manage` controls the screen wall; `/screen` is the public display
  client (also usable as an OBS browser source).
- Add/remove screens and assign content. Refresh the browser source or
  reconnect the client if a display goes stale.
- Use the static fallback page if the dynamic display cannot recover quickly.

## Live Broadcast (`/live`)

- `/live/manage` configures the live view; `/live` is the public broadcast
  page with variants:
  - `/live` — scoreboard view.
  - `/live/first-blood` — First Blood feed.
  - `/live/balloons` — balloon feed.
  - `/live/freeze-countdown` — freeze countdown.
  - `/live/statistics` — statistics.
- Live pages may be token-protected. Rotate rehearsal-shared tokens before the
  official contest.

## Balloons (`/balloon`)

- Balloon tasks are generated automatically for first accepted submissions
  using the configured per-problem balloon colors.
- Only official, non-star teams generate First Blood tasks; star teams can be
  configured to participate in balloons (and Resolver/awards) optionally.
- During the freeze interval, no new balloon tasks are generated.
- The workbench shows pending tasks and marks deliveries; keep the audit
  status accurate so no team is missed.

## Printing (`/printer`)

- The workbench lists print requests in `QUEUED`/`PRINTING`/`DONE`/`FAILED`
  states and sends them to the CUPS queue.
- Contestant-side limits: 20 KiB / 5 pages per request, one request per team
  every 10 minutes, 20 requests per team per contest.
- If the printer recovers after a failure, retry failed jobs; use the manual
  download fallback for urgent requests.

## Resolver (`/resolver`)

The Resolver reveals the frozen scoreboard step by step during the closing
ceremony. It is snapshot-based:

1. Generate a Resolver snapshot after the contest ends (or from the freeze
   state).
2. Preview the run.
3. Freeze the Resolver run for the ceremony.
4. Operate the ceremony: reveal, pause, resume, step back, and complete.
5. The current Resolver state and event history are persisted; a crashed
   ceremony can be resumed from the saved state.

The Resolver must never depend on live submission rows during the ceremony.
Rejudges or data fixes after snapshot generation require explicitly
regenerating the Resolver snapshot.

## Awards (`/awards`)

- Generate award recipient lists from the final scoreboard, with manual
  recipient workflows where needed.
- Award generation/freeze/export and recipient management are available under
  `/awards`; presentation control is at `/awards/presentation`, and the public
  display at `/awards/display`.
- Freeze the award list before the ceremony to keep it stable.

### Host Script (`/awards/host-script`)

- Edit and print the host cue sheet for a selected contest; the script is
  organized by award sections with cue text.
- The page shows the current cue and the next item, and stays in sync with
  the presentation status.
- Save edits with optimistic concurrency; if someone else changed the script,
  reload and retry.
- Print the script for the ceremony host.

## Event Checklist

- Before start: verify screens, live tokens, printer queue, balloon colors,
  and Resolver access.
- During: monitor Clarifications, balloons, printing, and judge queue.
- After end: drain the judge queue, generate the final scoreboard and Resolver
  snapshot, then run the Resolver and awards ceremonies.
- Record the incident log as described in
  [Troubleshooting](../../ops/troubleshooting.md) if anything fails.

## See Also

- [Administrator Guide](../admin/README.md) — permissions and contest setup.
- [Troubleshooting](../../ops/troubleshooting.md) — common failure checks.
