# User Guide

Role-based operating guides for people who use the platform without reading the
backend source: contestants, contest administrators, and on-site operations
staff (screens, live broadcast, balloons, printing, Resolver, and awards).

Each guide assumes the deployment described in
[`../ops/quickstart.md`](../ops/quickstart.md) is running. Configuration and
environment references are documented in
[`../ops/configuration.md`](../ops/configuration.md); common failure checks
live in [`../ops/troubleshooting.md`](../ops/troubleshooting.md).

## Sections

- [`contestant/`](contestant/README.md): registration/login, submitting code,
  reading verdicts, scoreboard and freeze rules, Clarifications, and printing.
- [`admin/`](admin/README.md): contests, teams, problems, staff accounts and
  permissions, rejudge, backups, and archiving.
- [`onsite/`](onsite/README.md): screen wall, live broadcast, balloons,
  printing workbench, Resolver ceremony, and awards presentation.

## Language

The English versions live in `docs/user/`. Chinese mirrors are under
`docs/zh-CN/user/` and cover the same content. Code blocks, commands, routes,
and permission codes are kept in English in both languages.

## Frontend Routes Used by These Guides

| Area | Routes |
| --- | --- |
| Contestant | `/contests`, `/contests/:contestId/{problems,submissions,clarifications,printing,scoreboard}`, `/profile`, `/change-password`, `/login`, `/register` |
| Admin | `/admin` (contests, problems, team import, announcements, rejudge, competition, practice, staff accounts, permissions) |
| On-site | `/judge` (Clarification workbench), `/printer`, `/balloon`, `/resolver`, `/awards`, `/screen/manage`, `/live/manage`, plus public displays `/screen`, `/live*`, `/resolver/display/:runId`, `/awards/display` |
