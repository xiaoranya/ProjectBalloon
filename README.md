# ProjectBalloon

ProjectBalloon is being reset as an idiomatic Rust workspace for an offline
XCPC/ICPC contest platform.

The active target layout starts at this workspace root:

```text
apps/api/             Axum modular-monolith API
apps/judge-worker/    Isolated judge worker
crates/domain/        Pure domain types and state transitions
crates/contracts/     Versioned external message contracts
crates/test-support/  Shared deterministic test fixtures
docs/                 Requirements, architecture, API, and operations
```

The previous implementation was fully migrated and removed; the contracts and
migration-compatibility tests under `crates/` and `docs/api/migration-matrix.md`
preserve the behavioral baseline.

## Bootstrap Checks

```text
cargo fmt --all --check
cargo check-all
cargo lint
cargo test-all
```

See `docs/README.md` and
`docs/architecture/ADR-002-rust-backend-reset.md` for migration boundaries.

The API runs embedded SQLx migrations on startup. `/livez` reports process
liveness without dependency access; `/api/health` reports PostgreSQL readiness
and returns HTTP 503 while the database is unavailable. See
`migrations/README.md` and `.env.example`.

For a fresh database, create the first super administrator exactly once after
setting the bootstrap variables from `.env.example`:

```text
cargo run -p project-balloon-api --bin bootstrap-admin
```

The command refuses to run after any user has been created. The initial
administrator must change the bootstrap password at first login.

## License

ProjectBalloon is licensed under the Apache License, Version 2.0. See
[`LICENSE`](LICENSE) for the complete text.
