# ProjectBalloon Documentation

This directory is the documentation root for the Rust implementation of
ProjectBalloon. The previous Java implementation has been migrated and removed;
the compatibility baseline remains in the reviewed contracts, migrations, and
API migration matrix.

## Status

- Architecture, requirements, operations, and the existing OpenAPI contract have
  been promoted to the workspace root.
- Backend implementation decisions now target Rust 2024, Tokio, Axum, SQLx,
  Redis, RabbitMQ, and S3-compatible object storage.
- `api/openapi.yaml` is the compatibility baseline captured from the previous
  implementation. It must not be regenerated as the new Rust contract until
  endpoint parity has been reviewed.
- The running Rust API serves its reviewed, code-generated OpenAPI 3.1 contract
  at `/api/openapi.json` and its vendored Swagger UI at `/api/docs`.
- References to Java in judge language lists mean contestant Java submissions,
  not the backend implementation language.

## Document Map

- `requirements/`: product and contest requirements. Kept in the repository for
  traceability; excluded from the published documentation site for now.
- `architecture/`: system boundaries, data ownership, judge, security, and
  architectural decisions, including `ADR-002-rust-backend-reset.md`.
- `api/`: externally observable HTTP contract and Rust implementation notes.
- `dev/`: Rust and frontend development rules.
- `ops/`: offline installation, operation, pressure testing, and recovery.
- `user/`: role-based user manuals for contestants, administrators, and on-site
  operators (new content; published as the "User Guide" section).

## Source Layout

```text
apps/api/              Rust modular-monolith API
apps/judge-worker/     Rust judge worker
crates/domain/         Pure domain types and state machines
crates/contracts/      Versioned AMQP and event wire contracts
crates/test-support/   Shared integration-test support
migrations/            SQLx PostgreSQL migrations
frontend/web/          Vue 3 frontend
deploy/judge/runtimes/ Contestant language images
deploy/                Offline deployment definitions
scripts/               Development and operations commands
apps/*/tests/          Application integration and acceptance tests
```

## Documentation Rules

- Treat the requirement document, database behavior, queue contracts, and
  reviewed OpenAPI operations as migration inputs, not as instructions to copy
  Java class structure.
- Update the relevant document in the same change as an architecture, contract,
  security, deployment, or operator-workflow change.
- Use repository-root-relative paths in commands and file references.
