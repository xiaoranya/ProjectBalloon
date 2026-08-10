# Scoreboard And Resolver

This document defines the P0 ICPC/XCPC scoreboard and resolver model.

## Scoreboard Principles

PostgreSQL stores authoritative contest data. Redis stores real-time scoreboard cache and must be rebuildable.

The public scoreboard must not calculate from the full submission table on every request. Updates should be incremental and scoped to the affected contest, team, and problem.

## ICPC/XCPC Ranking Rules

P0 ranking uses standard ICPC-style rules:

- More solved problems ranks higher.
- Lower penalty ranks higher.
- Penalty is accepted submission time plus wrong-submission penalty for solved problems.
- Wrong submissions on unsolved problems do not contribute to final penalty.
- First Blood is tracked per problem for official non-star teams only.
- Star teams are displayed and can participate in balloons, resolver, and awards depending on configuration.
- Group scoreboards are supported.

Tie-break (first version): solved count descending, penalty ascending, latest accepted submission time ascending, team display name ascending (case-insensitive, then case-sensitive fallback).

## Freeze Behavior

Before freeze:

- Public scoreboard shows real-time accepted and rejected effects.
- Admin scoreboard shows the same authoritative view.
- Balloon tasks are generated for first accepted submissions with configured colors.

After freeze:

- Public scoreboard includes submissions where `submitted_at <= freeze_at`.
- Public scoreboard hides submissions where `submitted_at > freeze_at`.
- Admin scoreboard continues to show the true state.
- Submissions still judge normally.
- Balloon tasks are no longer generated.
- Resolver uses freeze snapshot and final snapshot.

After contest end, the public board remains frozen and does not automatically reveal hidden submissions.

## First Version Status

The first implemented version includes:

- ICPC/XCPC ranking with explicit tie-break rules.
- Public and admin scoreboard variants with freeze semantics.
- Group scoreboard support.
- First blood per problem for official non-star teams only.
- Redis cache with after-commit invalidation.
- Scoreboard snapshot persistence.

The current implementation also includes:

- Resolver controls, immutable snapshots, event history, and current-state recovery.
- Step-by-step resolver reveal with pause, continue, back, and complete controls.
- SSE invalidation events for resolver, public scoreboard, presentation, announcements,
  and the staff clarification, balloon, and printing workbenches.
- Redis Pub/Sub fanout for SSE delivery across API instances, with polling fallback.
- Team-scoped SSE channels for submission status and private clarification replies.

Deferred to future versions:

- Rich animation commands beyond authoritative-state refresh hints.

## Data Model

Core tables referenced by the requirements:

- `submissions`
- `judgements`
- `runs`
- `scoreboard_snapshots`
- `resolver_runs`
- `resolver_snapshots`
- `resolver_events`
- `resolver_team_states`
- `resolver_current_state`

`scoreboard_snapshots` should store enough data to reconstruct a public or admin board without reading mutable submission history.

## Cache Strategy

Suggested Redis keys:

```text
xcpc:scoreboard:v1:{contestId}:{postgresRevision}:{variant}:{phase}:{selectorHash}
contest:{contestId}:first-blood
```

Cache values should include a version or generated timestamp so clients can avoid stale displays.

## Resolver Principles

Resolver must be snapshot based.

Required snapshots:

- Freeze-time public board.
- Final true board.
- Frozen submissions pending reveal.
- Team/problem state at each reveal step or replayable event stream.

Resolver must not depend on live submission rows during the official ceremony. Rejudging or data correction after snapshot generation should require explicit resolver snapshot regeneration.

## Resolver Flow

```text
Contest ends
  -> wait for judge queue to drain
  -> optional rejudge and validation
  -> generate final scoreboard
  -> generate resolver snapshot
  -> preview resolver run
  -> freeze resolver run for ceremony
  -> operate official resolver
  -> persist resolver current state and events
```

## Resolver Controls

P0 controls:

- Generate snapshot.
- Preview.
- Start official run.
- Next step.
- Pause.
- Continue.
- Step back.
- Auto play.
- Persist and recover current state.

Public resolver pages include screen and live variants. Operator APIs require the corresponding account permission.

## Validation Checklist

Before official resolver:

- Judge queue is empty.
- No `pending` or `judging` submissions remain unless intentionally excluded.
- Final admin scoreboard has been reviewed.
- Freeze snapshot and final snapshot are generated.
- Resolver preview matches expected final ranking.
- Awards generation uses the same final ranking source.
