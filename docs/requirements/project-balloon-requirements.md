# ProjectBalloon Product Requirements and Technology Selection

## 1. Project Goals

ProjectBalloon is an XCPC/ICPC contest platform that supports intranet deployment, covering the complete flow of pre-contest preparation, contest running, post-contest Resolver reveal, on-site operations, and data archiving.

The platform must meet the following goals:

- Support at least 500 teams participating online concurrently.
- Support 10,000 requests per minute.
- Support offline intranet deployment without depending on public services.
- Support standard XCPC/ICPC contest flows.
- Support freeze, Resolver-style reveal, printing, balloons, awards, large screens, and live-streaming assistance.
- Support automatic judging, real-time scoreboard updates, an admin console, and on-site operation monitoring.
- Support stable, recoverable, auditable official contest operation.

## 2. Use Scenarios

The platform is primarily aimed at the following scenarios:

- XCPC-style contests such as school, provincial, regional, and invitational contests.
- Official contests in intranet environments.
- Offline events that need on-site large screens, live streaming, balloons, printing, and award ceremonies.
- Contests that require pre-contest problem and account import, stable judging during the contest, and post-contest result export and archiving.

## 3. Core Scale Targets

| Metric | Target |
|---|---|
| Online teams | At least 500 |
| Online users | About 1,000-1,500 |
| Request volume | 10,000 requests/minute, about 167 RPS |
| Peak submissions | Designed for 100-300 per minute |
| Judging concurrency | Initially 30-60 concurrent judge slots |
| Deployment environment | Intranet, installable offline |
| Contest format | XCPC/ICPC for the first version |

## 4. Overall Architecture

The architecture uses a modular monolith business backend plus an independent judge cluster.

```text
Contestants / Administrators / Screens / Live
  ↓
Nginx
  ↓
Web frontend + API backend
  ↓
PostgreSQL / Redis / RabbitMQ / RustFS
  ↓
Judge Scheduler
  ↓
Judge Worker cluster
  ↓
Sandbox runtime
```

Confirmed technology choices:

| Module | Technology |
|---|---|
| Frontend | Vue 3 + TypeScript + Vite + Element Plus |
| Backend | Rust 2024 + Tokio + Axum |
| Database | PostgreSQL 16 |
| Cache | Redis 7 |
| Message queue | RabbitMQ 3 |
| Object storage | RustFS |
| Judge sandbox | rootless Podman + gVisor (`runsc`) + cgroups |
| Reverse proxy | Nginx |
| Deployment | systemd manages API/Worker + Docker/Podman judge sandbox |
| Automatic printing | CUPS |
| Monitoring | Prometheus + Grafana |
| Logging | Loki + Promtail |
| Image distribution | Judge Runtime offline image tar archives |
| Realtime events | SSE |
| Build and offline delivery | API/Worker binaries + frontend static files + Judge Runtime image tar archives + systemd/Nginx configuration + operation scripts |

## 5. Intranet Deployment Requirements

The platform must fit the on-site intranet environment of contests.

Deployment principles:

- No dependency on public CDN.
- No dependency on public object storage.
- No dependency on public container image registries.
- No dependency on public package management sources.
- No dependency on third-party login, SMS, cloud monitoring, or other external services.
- Frontend fonts, icons, code editor, MathJax/KaTeX, and syntax-highlighting resources are all localized.
- The Element Plus component library and its font and icon resources are shipped offline with the frontend build output, with no CDN dependency.
- Provide API/Worker binaries, frontend static files, Judge Runtime image tar archives, systemd units, Nginx configuration, and recovery scripts.
- PostgreSQL, Redis, RabbitMQ, RustFS, Nginx, CUPS, and observability components are provided and maintained by the deployer; the release package only provides application binaries, static files, and necessary configuration templates, and is not responsible for creating or upgrading external services.
- API and Judge Worker are managed by systemd; the Judge Worker starts isolated judge containers through the Docker/Podman socket.
- Service installation and first-round configuration are completed through `install.sh` in the release package root; running status is checked through systemd and health endpoints.

Standard deployment topology:

| Machine | Deployment content |
|---|---|
| gateway-01 | Nginx |
| app-01 | Frontend static assets, backend API |
| app-02 | Backend API, Judge Scheduler |
| data-01 | PostgreSQL, Redis, RabbitMQ, RustFS |
| judge-01 ~ judge-N | Judge Worker |
| backup-01 | Data backup, image backup, standby services |

Official contests use the standard topology above, with the judge service and database deployed on different machines.

Binary release package directory layout:

```text
bin/
  project-balloon-api
  project-balloon-judge-worker
  bootstrap-admin

web/
systemd/
config/
nginx/
scripts/backup/
install.sh
PACKAGE-SHA256SUMS

docs/
  install.md
  ops.md
  disaster-recovery.md
```

Judge Runtime images are not bundled with the binary release package. They are provided separately as the `project-balloon-<version>-<platform>-judge-images.tar.gz` archive and imported after extraction with `install.sh --judge-images judge-images`:

```text
judge-images/
  judge-runtime-*.tar
  SHA256SUMS
```

For the specific on-site deployment steps (role installation, environment configuration, first-start migrations, bootstrap-admin, health checks), see [`../ops/install.md`](../ops/install.md); daily operations after installation are covered in [`../ops/ops.md`](../ops/ops.md), and failure recovery in [`../ops/disaster-recovery.md`](../ops/disaster-recovery.md).

Judge Runtime image names must use fixed versions, never the `latest` tag, and must not depend on pulling images from the public network on site. Versioning, upgrades, and backups of base services are the deployer's responsibility.

## 6. Feature Scope

The first version is positioned as a complete XCPC on-site contest system, not a plain Online Judge.

Features are delivered in priority phases. P0 capabilities are mandatory for the first official contest:

- Account and team management.
- Contest management.
- Problem management.
- Submission and judging.
- ICPC/XCPC realtime scoreboard.
- Freeze.
- Resolver-style reveal.
- Clarification.
- Announcements.
- CUPS automatic printing of arbitrary text.
- Simple balloon task queue.
- Award rule configuration.
- Multi-screen display.
- Live realtime pages.
- Admin console.
- Monitoring, logging, backup, and recovery.

## 7. Accounts and Permissions

### 7.1 User Types

| Type | Description |
|---|---|
| Super administrator | Manages global configuration and all contests |
| Staff | Performs contest management, clarification, and on-site work through composable permissions |
| Team | Official contestant users |
| Star team | Different display marking, but participates in balloons, reveal, and awards |

Staff permissions are assigned directly to accounts, composable across contest management, clarification, printing, balloons, reveal, awards, screen, and live management. Contest-management permission and the manageable contest scope separately control "what can be done" and "in which contests".

### 7.2 Permission Requirements

- Support account login and password reset.
- Support batch import of team accounts.
- Support team school, members, seat number, group, and star-team marking.
- Support direct assignment and composition of staff account permissions.
- Support login logs and administrator action audit.

## 8. Contest Management

Contest management features include:

- Create contests.
- Set contest name, time, format, and visibility.
- Set start time, end time, and freeze time.
- Support contest pause and extension.
- Configure participating teams.
- Configure official teams, star teams, and other groups.
- Configure problem order and problem aliases.
- Publish announcements.
- Manage clarifications.
- Archive contests after they end.

The first version supports the ICPC/XCPC format.

## 9. Problem Management

Problem management features include:

- Create problems.
- Edit statements.
- Use Markdown as the statement editing format and PDF as the statement attachment format.
- Manage sample input/output.
- Upload problem attachments.
- Upload test data.
- Configure time and memory limits.
- Configure language restrictions.
- Configure Special Judge, priority P1.
- Configure problem balloon colors.
- Verify test data hashes.
- Freeze problem configuration before the contest.

Test data is stored in RustFS, not directly in the database.

## 10. Submission System

Submission features include:

- Teams select a problem and language and submit code.
- P0 supports C, C++, Java, and Python; P1 supports Go and Rust.
- Support submission status queries.
- Support submission detail views.
- Support compile error message views.
- Support submission rate limiting.
- Support code size limits.
- Support administrator rejudge of single or batch submissions.
- Support exporting all submission code and results.

Submission states include:

```text
pending
judging
accepted
wrong_answer
time_limit_exceeded
memory_limit_exceeded
runtime_error
compile_error
output_limit_exceeded
system_error
cancelled
```

## 11. Judge System

The judge system uses a scheduler service plus a Worker cluster.

Flow:

```text
User submits
  ↓
Write to submissions
  ↓
Send judge task to RabbitMQ
  ↓
Judge Worker pulls task
  ↓
Compile code
  ↓
Run test cases in sandbox
  ↓
Write back result
  ↓
Trigger scoreboard, balloon, and statistics updates
```

Judge system requirements:

- Support multiple Judge Workers.
- Support Worker health checks.
- Support task ACK, failure retry, and dead-letter queues.
- Support CPU time limits.
- Support memory limits.
- Support process count limits.
- Support file size limits.
- Support output size limits.
- Forbid runtime network access.
- Isolate the filesystem.
- Clean temporary directories.
- Record compile and run logs.
- Support local test-data caching.

Judge Workers should not be deployed in the same resource pool as the main business service and database.

## 12. Scoreboard System

The first version of the scoreboard system supports ICPC/XCPC rules.

Features include:

- Realtime scoreboard.
- Freeze.
- Administrator true board.
- Public board.
- First Blood.
- Wrong-submission penalty.
- Star-team display.
- Group scoreboards.
- Scoreboard export.
- Scoreboard snapshots.

The scoreboard must not be recomputed from the full submission table on every request. PostgreSQL holds authoritative data; Redis holds the realtime scoreboard cache.

After freeze:

- The public board hides the impact of post-freeze submissions.
- Administrators can view the true board.
- Resolver generates reveal data from the freeze snapshot and the final scoreboard snapshot.

## 13. Resolver-Style Reveal

The reveal follows the ICPC Resolver style.

Basic rules:

- Generate reveal data from the public board at freeze time and the final true board.
- Reveal teams one by one from low rank to high rank.
- Reveal post-freeze pending submissions.
- Dynamically update rankings based on reveal results.
- Star teams participate in the reveal.

Features include:

- Generate Resolver snapshots.
- Reveal preview.
- Official reveal.
- Host console.
- Next step, pause, continue, step back.
- Auto play.
- Reveal screen page.
- Reveal live page.
- Multi-group reveal, priority P1.
- Reveal state persistence.

Reveal data must be snapshot-based to avoid inconsistent results caused by rejudges or data changes during the official reveal.

Planned data:

```text
resolver_runs
resolver_snapshots
resolver_events
resolver_team_states
resolver_current_state
```

## 14. Clarifications and Announcements

Clarification features include:

- Teams ask questions to the jury.
- Questions can be linked to a problem or be general.
- Jury private replies.
- Jury public replies.
- Convert a reply into an announcement.
- Teams receive notifications of new replies.
- Support question rate limiting.

Announcement features include:

- Publish contest announcements.
- Pin announcements.
- Scheduled announcements, priority P1.
- Withdraw announcements.
- Announcement display on screens.
- Announcement ticker on live pages.

## 15. Automatic Printing Module

The printing module uses CUPS to automatically print arbitrary plain text pasted by teams.

### 15.1 Feature Scope

- Teams paste arbitrary text to initiate a print request.
- The backend validates text length, page count, and frequency limits.
- Automatically generates a PDF print file.
- Delivers to the intranet printer through CUPS.
- Print workbench to view the queue.
- Support failure retry.
- Support cancel and reject.
- Support manual download as a fallback.
- Support print audit.

### 15.2 Print Limits

| Limit | Default |
|---|---|
| Pages per request | At most 5 |
| Text size | At most 20KB |
| Frequency | At most 1 per team per 10 minutes |
| Total per contest | At most 20 per team |
| Content type | Plain text |

### 15.3 Print States

```text
requested
queued
printing
completed
failed
cancelled
rejected
```

### 15.4 Print Audit

Record:

- Contest ID.
- Team ID.
- Text content.
- Content hash.
- Page count.
- Printer ID.
- CUPS job ID.
- Request time.
- Completion time.
- Failure reason.
- Operator.
- Request IP.

## 16. Balloon System

The P0 balloon system implements a simple task queue; complex scheduling is deferred to P2.

### 16.1 Rules

- No balloon tasks are generated after freeze.
- Star teams participate in balloons.
- One balloon task is generated for each team's first AC on each problem.
- Problems must configure a balloon color.
- First Blood tasks need special marking.

Generation condition:

```text
Submission time < freeze time
AND result is AC
AND it is the team's first AC on that problem
AND the problem has a configured balloon color
```

### 16.2 Features

- Configure a balloon color per problem.
- Automatically generate balloon tasks.
- Display team, problem, color, and seat number.
- Mark First Blood.
- Staff manually update status.
- Support notes.
- Support task cancellation.
- Support balloon statistics.
- Support screen display.

### 16.3 States

```text
pending
preparing
delivering
delivered
cancelled
```

## 17. Award System

The award system supports generating award lists by ratio and by fixed count.

Star teams participate in awards by default, but each award can independently configure whether to include star teams.

Features include:

- Award configuration.
- Generate award lists by ratio.
- Generate award lists by fixed count.
- Generate award lists by rank range.
- Generate award lists from group scoreboards.
- First Blood awards, priority P1.
- Manual adjustment of award lists.
- Conflict detection and duplicate warnings.
- Freeze award lists.
- Export Excel/CSV.
- Export certificate data.
- Award screen.
- Award console, priority P1.

Example rules:

```text
Champion: rank 1
Gold: top 10%
Silver: next 20%
Bronze: next 30%
Best women's team: rank 1 on the women's board
First blood award: First Blood per problem
```

## 18. Screen System

The screen system must support multiple screens.

### 18.1 Screen Pages

| Page | Description |
|---|---|
| Contest overview | Team count, submission count, AC count, remaining time |
| Realtime scoreboard | Top N or paged carousel |
| First Blood | First-solve hints |
| Balloon statistics | Balloon colors, counts, pending deliveries |
| Announcements | Important announcement display |
| Freeze countdown | Pre-freeze notice |
| Resolver | Post-contest reveal |
| Awards | Display awards and winning teams |
| Data statistics | Submission trends, acceptance rate, language distribution |

### 18.2 Multi-Screen Control

Features include:

- Screen instance registration.
- Screen heartbeats.
- Console view of online screens.
- Remote page switching.
- Multi-screen grouping.
- Synchronized playback.
- Lock to a specific page.
- Disconnect-reconnect recovery.

Planned data:

```text
screen_instances
screen_groups
screen_playlists
screen_commands
screen_heartbeats
```

## 19. Live-Streaming Assistance

Live streaming displays public contest data in real time.

Live pages should be suitable for capture by OBS browser sources.

Features include:

- Realtime scoreboard live page.
- Resolver live page.
- Contest overview live page.
- First Blood popups.
- Balloon statistics page.
- Award page.
- Bottom announcement ticker.
- Live token authentication.
- Hide sensitive information such as accounts, IPs, submission code, and internal notes.

Example paths:

```text
/live/scoreboard
/live/resolver
/live/overview
/live/first-blood
/live/balloons
/live/awards
/live/ticker
```

Live and screens share the SSE realtime event channel.

## 20. Admin Console

The admin console includes:

- Dashboard.
- Team management.
- Contest management.
- Problem management.
- Submission management.
- Judging management.
- Scoreboard management.
- Resolver management.
- Clarification management.
- Announcement management.
- Print management.
- Balloon management.
- Award management.
- Screen management.
- Live management.
- Permission management.
- Audit logs.
- Data export.
- System health checks.

## 21. On-Site Operations

On-site operations capabilities include:

- Health check page.
- Service status checks.
- PostgreSQL status.
- Redis status.
- RabbitMQ queue backlog.
- RustFS status.
- Judge Worker online count.
- Judge queue length.
- API QPS and latency.
- HTTP 5xx error rate.
- Disk space.
- CPU and memory.
- Printer status.
- Screen online status.

The following must be provided:

- One-click check script.
- One-click backup script.
- Data recovery flow.
- Service restart script.
- Judge Runtime offline image tar archives.
- Configuration backup.
- Pressure-test scripts.

## 22. Security Requirements

### 22.1 Judge Security

- Forbid network access.
- Limit CPU, memory, process, file size, and output size.
- Isolate the filesystem.
- Do not mount the Docker socket.
- Do not use privileged containers.
- Clean temporary directories after every judging run.
- Mount test data read-only.

### 22.2 Web Security

- Prevent SQL injection.
- Prevent XSS.
- Prevent CSRF.
- Filter Markdown content.
- Restrict file upload types and sizes.
- Login rate limiting.
- Submission rate limiting.
- Print rate limiting.
- Administrator action audit.

### 22.3 Data Security

- Full backup before the contest.
- Scheduled backups during the contest.
- Persistent storage of submission code.
- Test-data hash verification.
- Object-storage backup.
- Leave traces for critical operations.

## 23. Data Storage Plan

Planned core tables:

```text
users
teams
team_members
contests
contest_teams
problems
contest_problems
submissions
judgements
runs
scoreboard_snapshots
clarifications
announcements
print_requests
balloon_tasks
balloon_colors
resolver_runs
resolver_snapshots
resolver_events
award_categories
award_rules
award_recipients
screen_instances
screen_groups
screen_commands
broadcast_tokens
audit_logs
```

File-type data is stored in RustFS:

- Problem attachments.
- Test data.
- Submission code archives.
- Compile logs.
- Judge logs.
- Export files.
- Print PDFs.

## 24. Priority Plan

### 24.1 P0 Must Complete

- Team accounts, login, permissions.
- Contest creation, time control, freeze.
- Problem management and test-data management.
- Code submission.
- Judge queue and Judge Worker.
- C/C++/Java/Python support.
- ICPC/XCPC scoreboard.
- First Blood.
- Clarification.
- Announcements.
- Admin console.
- Scoreboard export.
- Submission export.
- Resolver-style reveal.
- Reveal console.
- Reveal screen and live pages.
- CUPS automatic printing of arbitrary text.
- Print workbench and failure retry.
- Balloon color configuration.
- Automatic balloon tasks before freeze.
- Balloon workbench.
- Star teams participate in balloons, reveal, and awards.
- Award rules: ratio and fixed count.
- Award list export.
- Realtime screen pages.
- Basic multi-screen control.
- Safe live pages.
- Monitoring, health checks, backup and recovery.

### 24.2 P1 Enhanced Capabilities

- Enhanced CUPS printer status sync.
- Screen playlists.
- Multi-screen group synchronization.
- First Blood animations.
- Award console.
- Certificate data export.
- Host scripts.
- Resolver rehearsal mode.
- Enhanced balloon statistics.
- Worker local test-data cache.
- Contest cloning.

### 24.3 P2 Extended Capabilities

- OI/IOI format.
- Interactive problems.
- Output-only problems.
- Code similarity detection.
- Advanced live-stream packaging.
- Custom screen templates.
- Multi-venue synchronization.
- Public problem bank and training system.
- Multi-tenancy.

## 25. Pre-Contest Flow

The pre-contest flow should support:

```text
Create contest
  ↓
Import teams
  ↓
Configure problems
  ↓
Upload test data
  ↓
Configure language environment
  ↓
Configure balloon colors
  ↓
Configure printer
  ↓
Configure screen and live pages
  ↓
Import or generate accounts
  ↓
Test session
  ↓
Pressure test
  ↓
Pre-contest snapshot
  ↓
Freeze configuration
```

## 26. In-Contest Flow

Main in-contest flow:

```text
Teams log in
  ↓
View statements
  ↓
Submit code
  ↓
Enter judge queue
  ↓
Update submission result
  ↓
Update scoreboard
  ↓
AC before freeze generates a balloon task
  ↓
Teams can initiate text printing
  ↓
Jury handles clarifications
  ↓
Administrators monitor system status
```

After freeze:

- The public board display is frozen.
- The administrator true board keeps updating.
- No more balloons are generated.
- Submissions still judge normally.

## 27. Post-Contest Flow

Flow after the contest ends:

```text
Stop submissions
  ↓
Wait for the judge queue to drain
  ↓
Rejudge if necessary
  ↓
Generate the final board
  ↓
Generate Resolver snapshots
  ↓
Preview the reveal
  ↓
Official reveal
  ↓
Generate award lists
  ↓
Award screen display
  ↓
Export results, submissions, and logs
  ↓
Archive the contest
```

## 28. Pressure Tests and Drills

Pressure tests and failure drills are mandatory before an official contest. Pressure-test targets, the automated k6 suite, scenarios, success criteria, and the failure-drill checklist are covered in [`../ops/pressure-test.md`](../ops/pressure-test.md).

## 29. Version Boundary

First-version product goal:

```text
ProjectBalloon supports intranet deployment, covering pre-contest configuration, team management, problem management, submission and judging, realtime scoreboard, freeze, Resolver reveal, automatic printing, balloon delivery, award-list generation, multi-screen display, and live-streaming assistance.
```

The first version does not focus on fancy features, but on:

- Usable for official contests.
- Stable judging.
- Trustworthy scoreboard.
- Correct reveal.
- Printing and balloons that serve the venue.
- Safe screen and live display.
- Recoverable intranet deployment.
- Exportable and archivable data.
