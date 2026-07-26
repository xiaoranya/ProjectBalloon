# Git Workflow

This document defines the project Git workflow.

## Branches

Use a lightweight release workflow.

| Branch | Purpose |
|---|---|
| `main` | Releasable versions only |
| `develop` | Integration branch for normal development |
| `feature/*` | Feature development |
| `fix/*` | Normal bug fixes |
| `release/*` | Release stabilization and offline package validation |
| `hotfix/*` | Urgent fixes based on `main` |

Feature branch examples:

- `feature/auth-rbac`
- `feature/contest-problem`
- `feature/submission-judge`
- `feature/scoreboard-freeze`
- `feature/resolver`
- `feature/printing-balloon`
- `feature/screen-live`
- `feature/offline-package`

## Commit Messages

Use Conventional Commits.

Format:

```text
type(scope): summary
```

Common types:

- `feat`
- `fix`
- `docs`
- `chore`
- `test`
- `refactor`
- `perf`
- `build`
- `ci`

Common scopes:

- `frontend`
- `backend`
- `judge`
- `scheduler`
- `scoreboard`
- `resolver`
- `printing`
- `balloon`
- `awards`
- `screen`
- `live`
- `deploy`
- `docs`
- `database`

Examples:

```text
feat(backend): add team account import
feat(judge): add rabbitmq task consumer
fix(scoreboard): hide frozen submissions on public board
docs(deploy): add offline install procedure
build(release): generate offline package
```

## Release Flow

Normal release:

```text
develop
  -> release/X.Y.Z
  -> validate migrations, tests, compose, offline package
  -> merge to main
  -> tag vX.Y.Z
  -> merge back to develop
```

Official contest release candidate should include:

- Built frontend.
- Built backend image.
- Built judge worker image.
- Fixed-version Compose files.
- Offline image tar files.
- Offline Docker Engine and Compose Plugin packages.
- Checksums.
- Install and recovery docs.

## Tags

Product versions:

```text
v1.0.0
v1.0.1
v1.1.0
```

Optional offline package tags:

```text
offline-v1.0.0
```

Contest-specific internal tags may be used for traceability, for example:

```text
contest-2026-provincial-final
```

## Pull Request Checklist

Before merging:

- Tests relevant to the change pass.
- Database migrations are reversible or have a documented forward-only reason.
- Public/live/screen APIs do not expose sensitive data.
- Operational docs are updated for deployment or recovery changes.
- Offline deployment impact is considered.
- New secrets are represented only as `.env.example` placeholders.

## Protected Mainline Expectations

`main` should always represent a version that can be packaged or audited. Avoid direct commits to `main` except initial setup or emergency hotfixes when approved.
