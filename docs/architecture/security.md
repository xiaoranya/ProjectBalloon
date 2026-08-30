# Security

This document summarizes first-version security requirements.

## Threat Model

The platform runs untrusted contestant code and stores official contest data. Primary risks:

- Sandbox escape or host compromise.
- Data corruption during official contest.
- Unauthorized admin operation.
- Public leakage of source code, IPs, internal notes, or private clarifications.
- XSS through problem statements, announcements, or Markdown.
- Denial of service through excessive submissions, print requests, or refresh traffic.

## Judge Security

Required controls:

- Disable runtime network access.
- Enforce CPU, memory, process, file, and output limits.
- Isolate filesystem.
- Mount test data read-only.
- Clean temporary directories after every run.
- **Production / contest deployments**: Judge containers must not mount the Docker socket and must not run as privileged containers.
- **Development sandbox exception**: For local development and CI, the worker may run as a sibling-docker container (mounting `/var/run/docker.sock`) provided that (a) the worker process runs as a non-root user added to the `docker` group, (b) the worker never constructs shell or Docker CLI command strings from user-controlled input and uses only typed Docker API parameters through the Rust `bollard` client, and (c) every sandbox container is created with no network, a read-only root filesystem, `no-new-privileges`, a PID limit, a non-root UID/GID, and automatic cleanup. The threat model: user-submitted code is the untrusted boundary and stays inside the sandbox container; worker code is part of the trusted base.
- **Docker socket trust boundary**: a worker process with access to
  `/var/run/docker.sock` effectively holds host-root-equivalent privileges:
  anyone who escapes the worker (or the worker process itself when
  compromised) can launch or modify containers, mount arbitrary host paths,
  and read host files. Treat the worker host as inside the trust boundary —
  sandbox escape on a socket-mounted worker means an impact radius of the
  host's root user, not an isolated container. Keep such workers off
  database hosts and behind network segmentation accordingly.
- Keep judge workers separate from database hosts in official deployment.

Judge workers should run with the least host privileges compatible with the selected sandbox implementation.

## Web Security

Required controls:

- Hash new passwords with Argon2id. Accept legacy BCrypt hashes only during
  migration and upgrade them after a successful login.
- Direct, composable account permissions for all staff APIs.
- Apply login rate limits from shared PostgreSQL audit state so limits cannot be
  bypassed by switching API instances.
- Submission rate limit.
- Clarification rate limit.
- Print request rate limit.
- Protect every cookie-authenticated state-changing request with a signed
  double-submit CSRF token.
- SQL injection prevention through parameterized access.
- XSS prevention through output escaping and Markdown sanitization.
- Upload type, size, and extension validation.
- Strict separation of public/live/screen APIs from admin APIs.

## Browser Authentication

The Rust API uses opaque server-side sessions:

- The browser receives `PB_SESSION` as an `HttpOnly`, `SameSite=Lax` cookie.
- PostgreSQL stores only the SHA-256 digest of the random session token.
- Production sets `PROJECT_BALLOON_SECURE_COOKIES=true` and terminates TLS
  before requests reach the API.
- Login rotates the session and revokes any previous session presented by the
  browser.
- Logout deletes the server-side session before clearing the cookie.
- Administrative disabling, permission changes, and password resets revoke the
  affected user's sessions in the same transaction. Authentication also checks
  account state and an access fingerprint as defense in depth.
- A user flagged for password reset may access only the authentication flow
  until the password is changed; protected feature handlers must call
  `require_password_ready`.
- Feature handlers enforce authorization with `AuthContext::require_permission`;
  route visibility in the frontend is never an authorization boundary.

Before login, the frontend obtains a CSRF token from `GET /api/auth/csrf`. It
sends the returned value in `X-XSRF-TOKEN` for every state-changing request.
The readable `XSRF-TOKEN` cookie and header must match, and the server verifies
the token signature. `PROJECT_BALLOON_CSRF_SECRET` must be an independently
generated deployment secret. The checked-in development value is rejected
unconditionally unless `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET=true` is set for
local development, and it is never accepted together with secure cookies, so a
default deployment cannot sign CSRF tokens with a publicly known key.

## Staff Account Administration

Only an authenticated `SUPER_ADMIN` who has completed required password changes
may call `/api/admin/staff-accounts`.

- Staff creation normalizes usernames to lowercase and assigns the requested
  direct permission set; `SUPER_ADMIN` is a user type, not a role.
- `TEAM` accounts are managed by the team lifecycle and cannot be created or
  modified through the staff endpoint.
- New and administratively reset passwords always use Argon2id and require the
  user to choose a new password at the next login.
- A super administrator cannot remove or disable their own access.
- Staff access mutations use a PostgreSQL transaction-level advisory lock plus
  row locks. This serializes administrator creation and demotion, prevents lock
  inversion, and preserves at least one enabled super administrator under
  concurrent requests.
- User mutation, permission replacement, contest-scope cleanup, session revocation,
  and audit insertion commit atomically.

Contest administrator scope replacement also uses a single transaction. It
locks the target account, verifies every contest against non-deleted
authoritative rows, bulk-replaces assignments, and records the actor. Invalid
contest IDs must not partially clear or modify the previous scope.

Audit-log access is restricted to super administrators. Filter and pagination
values are bounded, ordering choices are allow-listed, and all SQL values remain
parameters. Audit responses may contain user IDs and request IPs and must never
be exposed through public, live, screen, or contestant APIs.

## Contest Access

Contest reads use one visibility policy in the database query so pagination
counts and returned rows cannot disagree:

- public contests are readable anonymously;
- team and contest-manager access is resolved from authoritative
  assignment tables;
- explicitly assigned operational permissions may read private contests without
  receiving configuration permissions;
- inaccessible and nonexistent contest details both return `CONTEST_NOT_FOUND`
  to avoid private-contest enumeration.

Contest managers are authorized again inside mutation transactions. Permission
changes revoke active sessions, and contest assignments are checked in the same
transaction as each mutation.
Only super administrators may create contests or inspect soft-deleted records.

## Team Identity

Team authorization is identifier-based. An authenticated contestant account is
linked to exactly one team through `team_accounts`; mutable usernames, display
names, team names, and schools are never treated as identity. Team-scoped
queries resolve and constrain this relation in PostgreSQL before returning
private contest or realtime data.

Passwords are accepted only on account creation, batch import, and explicit
reset. They are Argon2-hashed before persistence, never stored or returned in
plaintext, and password reset revokes the account's existing sessions.

## Sensitive Data Rules

Never expose these through public scoreboard, screen, or live APIs:

- Submission source code.
- Login names unless intended for display.
- Password hashes.
- Internal notes.
- Private clarification replies to other teams.
- Request IPs.
- Admin operation logs.
- Backup paths and credentials.

## Token And Secret Handling

- Commit only `.env.example` files.
- Store real `.env` files outside Git.
- Do not log raw session or CSRF tokens.
- Rotate live broadcast tokens before official contest if rehearsal tokens were shared.
- Use separate credentials for PostgreSQL, Redis, RabbitMQ, RustFS, Grafana, and application services.
- Do not print secrets in health check output.

## Printing Security

P0 printing accepts arbitrary plain text, not arbitrary files.

Controls:

- Maximum text size: 20 KB by default.
- Maximum pages: 5 by default.
- Per-team frequency: once per 10 minutes by default.
- Per-team total: 20 times by default.
- Audit text hash, page count, team, operator, printer, CUPS job ID, request IP, and failure reason.

## Operational Security

- Perform full backup before official contest.
- Perform scheduled backups during contest.
- Verify restore procedure before contest day.
- Keep offline image tar files and deployment package checksums.
- Restrict shell access to deployment hosts.
- Keep backup copies on `backup-01` or external media.
