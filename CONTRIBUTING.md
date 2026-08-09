# Contributing to ProjectBalloon

ProjectBalloon is an offline-first XCPC/ICPC contest platform. The active
implementation is the Rust workspace at the repository root. Compatibility
behavior is preserved through API contracts and migration tests.

Before opening a pull request, run:

```text
cargo fmt --all --check
cargo check-all
cargo lint
cargo test-all
python3 scripts/check-api-compat.py --check
```

Frontend changes should also pass `npm ci`, `npm test -- --run`, and
`npm run build` from `frontend/web`. Docker-dependent integration tests are
available through `scripts/test/docker-integration.sh`.

Use focused commits and include tests for behavior changes. Do not commit
credentials, local `.env` files, `target/`, `node_modules/`, or generated
release artifacts.
