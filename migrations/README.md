# SQLx Migrations

Alpha layout: the entire schema lives in a single SQLx migration:

- `0001_initial.sql` — full fresh-install schema (all tables, indexes,
  constraints, functions, triggers, and seed data).

The file is the concatenation of the original timestamped migration history
(`20260719*` through `20260809*`) in application order. Each section banner
names the source migration so the history stays traceable. SQLx runs the whole
file inside one transaction on a fresh database.

## Rules

- Alpha: the single migration may be edited destructively, because deployments
  are expected to drop and recreate (or restore) databases rather than upgrade
  in place.
- Before the first non-alpha release, freeze `0001_initial.sql` and add new
  forward migrations with a sortable numeric prefix instead of editing it.
- Test the migration against the supported PostgreSQL major version before
  merge.
- Keep seed data deterministic; do not embed generated IDs or timestamps unless
  they are part of the required contract.

## Existing Installations

This consolidation is destructive. Databases created from the previous 50-file
migration chain must be dropped and recreated (or restored from a backup)
before running the new migration; in-place upgrades are not supported.
