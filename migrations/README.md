# SQLx Migrations

`20260719000000_initial_baseline.sql` is the fresh-install baseline for the
Rust reset.

`20260719010000_auth_sessions.sql` is the first Rust-owned forward migration.
It adds opaque browser sessions and the audit index used by distributed login
rate limiting. Session tokens are never stored in plaintext.

`20260719020000_super_admin_role.sql` moves the final runtime-created built-in
role into deterministic migration data, allowing staff accounts to be managed
without application startup seed hooks.

`20260719030000_admin_query_indexes.sql` adds deterministic query support for
reverse-chronological audit browsing and user-type/username administration
lists.

`20260719040000_contest_integrity.sql` moves active contest-name uniqueness
from a race-prone application check into PostgreSQL while preserving name reuse
after soft deletion. It also indexes the reviewed contest list orderings.

`20260719050000_realtime_outbox.sql` creates the durable publication boundary
for contest realtime events. Business transactions insert `PENDING` rows;
delivery state belongs to the dispatcher and is never inferred from Redis.

`20260719060000_realtime_team_scope.sql` adds the nullable team recipient and a
database check that makes TEAM events private while forbidding recipient IDs on
PUBLIC and STAFF events.

`20260719070000_team_accounts.sql` replaces display-name-based contestant
identity with an explicit one-to-one user/team relation. It also adds optimistic
team versions, active-name uniqueness, reviewed participation types, and roster
query indexes.

`20260719080000_problem_integrity.sql` adds optimistic problem versions,
database-enforced positive resource limits, and active-only slug uniqueness so
a slug can be reused after soft deletion.

`20260719090000_contest_problem_integrity.sql` enforces the reviewed alias and
display-order ranges. It makes per-contest display-order uniqueness deferrable
so a complete reorder can exchange positions atomically without exposing an
invalid committed state.

`20260719100000_problem_testdata_versions.sql` adds immutable per-problem
test-data version metadata. Fresh uploads retain every version and atomically
advance the compatibility pointer on `problems`; legacy current pointers are
backfilled without inventing an unknown object byte count.

`20260719110000_testdata_case_count.sql` records the number of validated,
root-level `.in`/`.out` pairs for each new version. Legacy rows remain nullable
because their archives were not inspected during ledger migration.

`20260719120000_submission_integrity.sql` adds source SHA-256 metadata, the
reviewed 64 KiB source bound, submission-outbox state constraints, and the
recent-team index used by exact per-team submission rate limiting.

`20260722130000_object_storage_cleanup.sql` adds the durable, leased cleanup
queue for RustFS/S3 objects whose immediate compensation or metadata-driven
deletion failed. Unique bucket/key identity and expiring leases make retries
idempotent and safe across multiple API instances.

`20260719130000_submission_outbox_lease.sql` adds multi-instance publication
leases and dispatch scheduling. Expired `PUBLISHING` rows are reclaimable, so
an API crash cannot strand a Judge task before RabbitMQ confirmation.

`20260719140000_judge_results.sql` adds the immutable result-message idempotency
key and enforces one run per test index for each judgement.

`20260719150000_judge_workers.sql` stores RabbitMQ-delivered Worker heartbeat,
capacity, occupancy, language, runtime, and sandbox metadata for authoritative
online-health projection.

`20260719160000_icpc_scoreboard_projection.sql` adds authoritative per-team/problem
ICPC cells and aggregate rows. Result transactions rebuild the affected cell from
active judgements, making duplicate delivery safe and PostgreSQL the rebuildable source.

`20260719170000_scoreboard_snapshot_integrity.sql` adds snapshot filter metadata,
payload SHA-256, creator identity, deterministic version uniqueness, and a database
trigger that rejects mutation or deletion of immutable scoreboard snapshots.

`20260719180000_scoreboard_cache_revision.sql` adds a PostgreSQL-authoritative scoreboard
revision and database triggers that advance it when projections, rosters, contest problems,
contest timing, or team display data change. Redis keys include this revision, so a cache outage
cannot resurrect stale standings.

`20260719190000_submission_rejudge.sql` adds the terminal `CANCELLED` submission-Outbox state so
a rejudge can retire an unpublished or leased old Judge task without making it retryable.

`20260719200000_batch_rejudge_recovery.sql` adds batch task/item state constraints, item leases,
attempt counters, and a unique judgement-to-item idempotency anchor for crash-safe recovery.

`20260719210000_clarification_integrity.sql` closes clarification scope, status, reply, close, and
text-length states; records the closing actor/time; and adds the exact team rate-limit index.

`20260719220000_announcement_integrity.sql` normalizes legacy enum casing, closes announcement
text and lifecycle states, and prevents one clarification from producing duplicate announcements.

`20260719230000_print_request_integrity.sql` closes print size, page, hash, PDF, lifecycle, failure,
and completion states and adds the deterministic queued-work index.

`20260719240000_cups_delivery_recovery.sql` adds multi-instance CUPS delivery leases, attempt and
submission tracking, deferred cancellation, failure diagnostics, and the recovery claim index.

`20260719250000_balloon_task_integrity.sql` closes the balloon task lifecycle, adds optimistic
versions and reopen tracking, and enforces one First Blood task per contest problem.

`20260719260000_resolver_integrity.sql` anchors each Resolver run to immutable public/final
scoreboard snapshots, closes its recoverable state machine, preserves pending submissions and
step history, and prevents official source data from changing after run creation.

`20260719270000_award_integrity.sql` closes award categories and rules, binds generated recipients
to the official Resolver final snapshot, adds an explicit draft/frozen award set, and preserves
manual adjustments with optimistic versions.

`20260801010000_realtime_outbox_lease.sql` adds owner-bound realtime publication leases so an
expired API instance cannot overwrite a newer dispatcher result, while preserving one final
delivery attempt after an expired lease reaches the retry limit.

It was generated from the effective PostgreSQL schema after applying the
previous V001-V031 migration history, then stripped of `pg_dump` session and
client meta-commands. The baseline was verified against the previous result:

- 47 business tables;
- 441 business columns;
- 464 table constraints;
- 130 PostgreSQL indexes, including constraint indexes;
- 9 built-in operator roles;
- no column-definition differences.

## Rules

- Never edit an applied migration.
- Generate forward migrations with a sortable timestamp prefix.
- Run migrations through the API's embedded SQLx migrator in normal deployment.
- Test migrations against the supported PostgreSQL major version before merge.
- Keep seed data deterministic; do not embed generated IDs or timestamps unless
  they are part of the required contract.

## Existing Installations

In-place upgrades from the previous implementation are not supported by this
release. Do not apply the Rust baseline to an existing Java database. Deploy a
fresh Rust database and use a separately reviewed export/import process if
historical records must be retained; that process is outside the Rust migration
chain and does not make the old database upgradeable in place.
