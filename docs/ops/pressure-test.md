# Pressure Test

This document defines rehearsal pressure tests required before official deployment.

## Targets

Requirement targets:

- 1,500 user login simulation.
- 500 teams viewing problem statements concurrently.
- 500 teams refreshing scoreboard concurrently.
- 100 to 300 submissions per minute.
- Judge queue accumulation and drain test.
- Resolver page pressure test.
- Multi-screen connection test.
- Print task pressure test.

## Test Environment

Pressure tests should run on hardware close to the official deployment. If reduced hardware is used, record the difference clearly.

Required test data:

- At least 500 teams.
- Contest with realistic problem count.
- C, C++, Java, and Python submissions.
- Mixed accepted, wrong answer, compile error, runtime error, and timeout cases.
- Configured balloon colors.
- Configured printer or printer mock.
- Screen and live clients.

## Automated k6 Suite

The repository provides three profiles in `scripts/pressure/k6`:

| Profile | Login users | Concurrent readers | Submission rate | Screen clients | Duration |
| --- | ---: | ---: | ---: | ---: | ---: |
| `smoke` | 1 | 1 | one write when enabled | 1 | 10 seconds |
| `rehearsal` | 300 | 200 | 120/minute | 20 | 10 minutes |
| `full` | 1,500 | 500 | 300/minute | 100 | 30 minutes |

`rehearsal` is intended for tuning on reduced hardware. `full` matches the
requirement targets. Non-smoke profiles refuse to start unless the account file
contains at least as many unique accounts as the login target.

Prepare an untracked JSON file outside the repository:

```json
[
  {"username": "team-001", "password": "rehearsal-only-password"},
  {"username": "team-002", "password": "rehearsal-only-password"}
]
```

Run a read-only smoke test against a local environment:

```bash
BASE_URL=http://127.0.0.1:8080 \
CONTEST_ID=1 \
ACCOUNTS_FILE=/secure/path/accounts.json \
scripts/pressure/run-k6.sh
```

Run the authorized full rehearsal, including submissions and print requests:

```bash
BASE_URL=https://rehearsal.example.internal \
CONTEST_ID=42 \
PROFILE=full \
ENABLE_WRITES=true \
ACCOUNTS_FILE=/secure/path/accounts.json \
XCPC_PRESSURE_TARGET_ACK=I_UNDERSTAND_THIS_GENERATES_LOAD \
scripts/pressure/run-k6.sh
```

The non-local acknowledgement is deliberately verbose to prevent accidental
load against an unapproved target. Never use production credentials. The
account fixture, submission source, cookies, and passwords are not written to
the report. The default report is `build/reports/k6/summary.json`.

Optional controls:

- `P95_LATENCY_MS` sets the login/read p95 threshold (default `1500`).
- `MAX_FAILURE_RATE` sets the HTTP failure-rate threshold (default `0.01`).
- `SUBMISSION_LANGUAGE` and `SUBMISSION_SOURCE` select the generated submission.
  Set `SUBMISSION_LANGUAGE=mixed` to rotate C, C++, Java, and Python submissions;
  when `SUBMISSION_SOURCE` is omitted, the suite supplies a valid source for each
  language.
- `ENABLE_SUBMISSIONS=false` or `ENABLE_PRINTS=false` isolates one write workload
  while diagnosing a failed smoke run. Both are enabled by default whenever
  `ENABLE_WRITES=true`.
- `REPORT_DIR` selects the summary output directory.
- `SAVE_K6_JSON=true` also writes the raw k6 metric stream to
  `REPORT_DIR/metrics.json`; compress it after the run because rehearsal and
  full profiles can produce large files.
- `DURATION` overrides the selected profile's workload duration for a bounded
  diagnostic run. Omit it for official rehearsal or full evidence.
- `RESOLVER_RUN_ID` enables the Rust public Resolver state workload. It is
  skipped when no completed or active rehearsal run is prepared.

The runner uses a locally installed `k6` binary when available and otherwise
uses the pinned `grafana/k6:0.57.0` Docker image with host networking. The Rust
API returns the rendered contestant statement as part of the contest-scoped
problem list, so that request is the statement workload; the super-admin-only
global problem endpoint must not be used by team accounts.

Before running `rehearsal` or `full`, confirm that the contest is running, every
account is assigned to it, at least one problem is visible, judge workers are
healthy, the printer uses a test queue or mock, and metrics dashboards are
recording. The suite fails on any dropped arrival-rate iteration, a check rate
at or below 99%, an HTTP failure rate at or above 1%, or a p95 threshold breach.

## Scenarios

Login test:

```text
Stagger 1,500 unique-account logins over one minute
Measure success rate, p95 latency, 5xx rate, database connections
```

Non-smoke profiles distribute the login target evenly over one minute. This
models the contest login wave without turning a scheduler-synchronized BCrypt
spike into the latency measurement. The remaining workloads begin after a
30-second settling interval. Submission and print arrival-rate executors
pre-allocate enough VUs for authentication and tail latency so that runtime VU
initialization does not create false dropped-iteration failures.

Problem statement test:

```text
500 teams request problem list and statements
Include attachment reads from RustFS if applicable
```

Scoreboard test:

```text
500 teams poll or receive scoreboard updates
Verify Redis cache hit behavior and API latency
```

Submission test:

```text
Generate 100 to 300 submissions per minute
Track queue depth, wait time, judge duration, system_error rate
```

For the judge accumulation/drain test, record queue depth immediately before
enabling writes, at the end of the submission window, and once per minute until
depth returns to its starting value. The k6 request summary proves accepted
submission volume; RabbitMQ and worker metrics prove that no task was lost.

Resolver test:

```text
Generate resolver snapshot
Connect screen and live pages
Replay official reveal sequence
Measure UI update latency and server errors
```

Print test:

```text
Generate print requests with configured limits
Verify rate limits, queue state, failure retry, and manual fallback
```

## Success Criteria

Minimum success criteria:

- API 5xx rate remains near zero under expected load.
- Login and normal page p95 latency remain acceptable for contest use.
- Scoreboard updates are not computed from full submission history per request.
- Judge queue drains after submission burst.
- No lost judge tasks.
- No unexpected `system_error` burst.
- Public/live/screen pages do not expose sensitive fields.
- Backup and health check still work during or after load.

## Failure Drill

During rehearsal, intentionally test:

- API restart.
- Judge worker stop and restart.
- RabbitMQ restart.
- Redis restart or clear followed by cache rebuild.
- PostgreSQL backup and restore on test data.
- RustFS temporary read failure.
- Printer offline.
- Screen disconnect and reconnect.

Record observed recovery time and any manual commands required.

Do not automate the failure drill in the load generator. An operator should
inject one failure at a time, annotate the dashboard timestamp, wait for
recovery, and only then continue. Save the k6 JSON summary together with metric
screenshots, queue-depth observations, hardware differences, and the incident
timeline as the rehearsal evidence.
