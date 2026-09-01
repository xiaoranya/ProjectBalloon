<!-- Title: conventional commit style, e.g. "feat(api): ...", "fix(judge-worker): ..." -->

## Summary

<!-- What this PR changes and why. One or two paragraphs. -->

## Related issues

<!-- e.g. "Fixes #123" or "Refs #456". Remove if none. -->

## Type of change

- [ ] Bug fix (non-breaking behavior fix)
- [ ] New feature
- [ ] Refactor (no behavior change)
- [ ] API / contract change (`crates/contracts`, `docs/api/`)
- [ ] Judge sandbox or runtime image change (`apps/judge-worker`, `deploy/judge/`)
- [ ] Deployment / operations (`deploy/`, `scripts/`, CI)
- [ ] Documentation or tests only

## Checklist

General:

- [ ] Commits are focused and follow the conventional commit style.
- [ ] No credentials, local `.env` files, `target/`, `node_modules/`, or generated release artifacts are committed.
- [ ] Behavior changes include tests.
- [ ] Docs updated when behavior, configuration, or deployment changes (`docs/`, including `docs/zh-CN/` counterparts).

Rust workspace (required unless docs-only):

- [ ] `cargo fmt --all --check`
- [ ] `cargo check-all`
- [ ] `cargo lint`
- [ ] `cargo test-all`
- [ ] `python3 scripts/check-api-compat.py --check`
- [ ] OpenAPI document is fresh: `cargo run -p project-balloon-api --bin export-openapi > docs/api/openapi.runtime.json` produces no `git diff`
- [ ] `cargo sqlx prepare --check --workspace` passes (only needed when `sqlx::query!`-style macros are used)

Frontend (required when `frontend/web` changed):

- [ ] `npm ci`
- [ ] `npm test -- --run`
- [ ] `npm run build`

Docker-backed integration (required when sandbox, runtime images, or deploy behavior changed):

- [ ] `scripts/test/docker-integration.sh`
