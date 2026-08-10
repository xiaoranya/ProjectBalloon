# Git Workflow

This document defines the project Git workflow.

## Branches

Use a lightweight release workflow.

| Branch | Purpose |
|---|---|
| `main` | Releasable versions only |
| `feature/*` | Feature development |
| `fix/*` | Normal bug fixes |
| `release/*` or `chore/release-*` | Release stabilization and binary package validation |
| `hotfix/*` | Urgent fixes based on `main` |
| `deps/*`, `chore/*`, `refactor/*`, `docs/*` | Dependency updates, maintenance, refactors, and documentation |

Feature branch examples:

- `feature/competition-mode-ip-login`
- `feature/permission-role-migration`
- `fix/sqlx-0.9-update`
- `deps/frontend-security-updates`
- `refactor/split-large-files`
- `docs/vitepress-site`

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
docs(deploy): add binary install procedure
build(release): generate binary package
```

## Release Flow

Normal release:

```text
feature/* or fix/* branch
  -> pull request reviewed and merged to main
  -> release/X.Y.Z (or chore/release-*) from main
  -> validate migrations, tests, binary package,
     and optional compatibility Compose configuration
  -> tag vX.Y.Z
```

Official contest release candidate should include:

- Built frontend.
- Built API and Judge Worker binaries.
- Built frontend static files.
- Fixed-version Judge Runtime image tar files (published as a separate judge-images archive).
- External-service prerequisites documented separately.
- Checksums.
- Install, backup, and recovery docs.

## Tags

Product versions:

```text
v1.0.0
v1.0.1
v1.1.0
```

Pre-release versions use SemVer suffixes and are published as GitHub
pre-releases, for example `v0.1.0-alpha.1`.

Optional contest package tags:

```text
contest-package-v1.0.0
```

Contest-specific internal tags may be used for traceability, for example:

```text
contest-2026-provincial-final
```

## Pull Request Checklist

Before merging:

- Tests relevant to the change pass.
- Database migrations are reversible or have a documented forward-only reason.
  In alpha, edits to the single `migrations/0001_initial.sql` are destructive
  and must be called out in the change description.
- Public/live/screen APIs do not expose sensitive data.
- Operational docs are updated for deployment or recovery changes.
- Binary deployment impact is considered.
- New secrets are represented only as `.env.example` placeholders.

## Protected Mainline Expectations

`main` should always represent a version that can be packaged or audited. Avoid direct commits to `main` except initial setup or emergency hotfixes when approved.
