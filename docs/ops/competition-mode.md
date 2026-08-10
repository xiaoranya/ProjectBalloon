---
title: Competition Mode
description: How to run ProjectBalloon in competition deployment mode with
  static-IP workstations and pairing-code login for an on-site contest.
---

# Competition Mode

This page is for operators who run ProjectBalloon in `competition` deployment
mode: an on-site contest where each seat is a fixed workstation and teams log
in with pairing codes instead of account passwords.

## What Competition Mode Does

Set `PROJECT_BALLOON_DEPLOYMENT_MODE=competition` in the environment file and
restart the API service:

- Enforces non-overlapping contest schedules: no two contests may overlap, and
  only one contest may be active at a time.
- Enables IP-bound workstation pairing: teams sign in at `/login` with a
  pairing code from their assigned workstation.
- Disables the daily features: public problem bank, training plans, daily
  practice, virtual contests, self-registration, and admin practice pages.

## Prepare Workstations

- Give every contestant machine a static IP address that the browser can
  reach.
- Record the seat number and IP of each machine; you will register them in the
  admin UI.

## Set Up Binding

1. Sign in as an administrator with `CONTEST_MANAGE` and open
   `/admin/competition`.
2. Register each workstation: IP address, seat number, and optional label.
3. Load the bindings for the contest.
4. Bind a workstation to a team; the team must already be in the contest
   roster.
5. Copy the pairing code shown once and hand it to the seat.

See the [Administrator Guide](../user/admin/README.md) for the full workflow.

## Verify Before the Contest

- Confirm exactly one contest is `RUNNING`/`PAUSED` and inside its schedule.
- From a test workstation, open `/login`, choose the pairing-code mode, and
  sign in.
- Confirm the signed-in team matches the seat and the contest is the expected
  one.
- After rehearsal, rotate the pairing code for every seat that used a shared
  code; rotation invalidates existing sessions.
- Confirm the schedule integrity check passes (no overlapping contests).

## Runtime Behavior

- Pairing codes are shown only at bind or rotate time; they are never
  retrievable later.
- A session is bound to the workstation IP; requests from a different IP are
  rejected, so the team must log in from the assigned machine.
- Sessions end when the contest ends.
- Rotating a code or revoking a binding invalidates that seat's sessions
  immediately.

## Troubleshooting

- "No contest is currently active": the contest is not within its schedule or
  is not `RUNNING`/`PAUSED`.
- "This IP address is not bound": the workstation is not registered/enabled or
  has no binding for the active contest.
- "Pairing code is invalid": re-check the code; if it was shared, rotate it.

## See Also

- [Operations](ops.md) — routine on-site operations.
- [Configuration Reference](configuration.md) — deployment mode variable.
- [Administrator Guide](../user/admin/README.md) — workstation binding UI.
