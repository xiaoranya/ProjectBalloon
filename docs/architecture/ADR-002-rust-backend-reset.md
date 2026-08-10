# ADR-002: Rust Backend Reset

## Status

Accepted for the ProjectBalloon reset.

## Context

The previous implementation was a Java/Spring modular monolith with an embedded
judge scheduler and a separately deployed judge worker. Its externally visible
behavior includes a large HTTP API, PostgreSQL schema, SSE channels, RabbitMQ
messages, S3-compatible objects, and operator workflows.

The reset is intended to produce an idiomatic Rust system. Copying the previous
controller/service/repository/entity class graph would preserve implementation
accidents, create unnecessary traits and dynamic dispatch, and hide transaction
and concurrency boundaries.

## Decision

Use one Cargo workspace with:

- an Axum API modular monolith on Tokio;
- the scheduler and transactional-outbox dispatcher hosted by the API;
- a separately deployable Tokio judge worker;
- pure shared domain types and versioned wire contracts in a small number of
  library crates;
- SQLx with explicit PostgreSQL queries and transactions;
- Lapin for RabbitMQ, `redis` for Redis, and `object_store` for RustFS;
- Utoipa for the reviewed OpenAPI contract generated from Rust code.

The Vue frontend, PostgreSQL as source of truth, Redis as rebuildable state,
RabbitMQ topology, RustFS object semantics, offline deployment model, and
production sandbox isolation remain architectural constraints.

## Module Rules

- Business features live below `apps/api/src/features/`.
- Infrastructure adapters live below the owning application unless they are
  genuinely shared by multiple binaries.
- `crates/domain` has no framework or infrastructure dependencies.
- `crates/contracts` owns AMQP/SSE payload versions; it does not own business
  orchestration.
- Traits represent a meaningful replaceable boundary, not every database table.
- HTTP DTOs, database rows, domain values, and queue messages are not treated as
  one universal entity type.
- Official state transitions are expressed with enums and checked transition
  functions.

## Compatibility Boundary

The following are migration inputs that require explicit compatibility review:

- `docs/api/openapi.yaml`;
- the effective PostgreSQL schema and constraints;
- RabbitMQ exchange, queue, routing-key, ACK, retry, and dead-letter behavior;
- JSON field names and enum values in judge task/result messages;
- SSE channel authorization and public-data filtering;
- RustFS bucket/key conventions and object hashes;
- backup, restore, offline installation, and health-check behavior.

Compatibility is assessed by behavior and fixtures, not by matching previous
source filenames or classes.

## Persistence Decision

For a fresh installation, consolidate the previous schema history into a
reviewed SQLx baseline migration, then create immutable forward migrations.

Update (2026-08): while the project is in alpha, the baseline is a single
consolidated SQLx migration (`migrations/0001_initial.sql`) and may be edited
destructively; it will be frozen before the first non-alpha release, after
which forward migrations become immutable.

For an installation containing official data, the Rust reset does not promise
an in-place upgrade. Deploy the Rust baseline into a fresh database; any
historical-data export/import must be designed and reviewed separately from
the SQLx migration chain.

Database writes coupled to RabbitMQ publication use a transactional outbox.
Consumers are idempotent and ACK only after a durable result, retry, or dead
path is established.

## Consequences

- Rust code is designed around ownership, typed state, explicit transactions,
  bounded concurrency, and cancellation rather than framework annotations.
- The API remains one deployable unit, avoiding premature distributed
  transactions and operational overhead.
- The judge worker remains isolated because its resource use and threat model
  differ from the business API.
- Contract fixtures and integration tests preserve the reviewed behavior after
  removal of the previous implementation.
- Build and dependency artifacts must be cached and packaged for fully offline
  installation.
