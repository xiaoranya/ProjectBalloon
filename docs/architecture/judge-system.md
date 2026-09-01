# Judge System

The judge system is split into a scheduler and worker cluster. It is designed to support 100 to 300 submissions per minute and an initial 30 to 60 concurrent judge slots.

## Responsibilities

The API backend owns submission validation and durable submission creation. The judge scheduler and workers own execution.

Scheduler responsibilities:

- Publish and coordinate judge tasks.
- Track queue depth and worker health.
- Support rejudge requests.
- Apply retry and dead-letter policies.
- Avoid overloading workers or data services.

Worker responsibilities:

- Pull tasks from RabbitMQ.
- Fetch source code and test data.
- Compile source code.
- Run test cases in sandbox.
- Enforce resource limits.
- Upload logs and artifacts.
- Report final judgement.

## Task Lifecycle

```text
pending submission
  -> judge task published
  -> worker receives task
  -> submission marked judging
  -> compile
  -> run test cases
  -> aggregate result
  -> persist judgement and runs
  -> publish scoreboard/event updates
```

## Queue Design

Recommended RabbitMQ queues:

- `judge.tasks`: normal judge tasks.
- `judge.retry`: delayed or retry tasks.
- `judge.dead`: tasks that exhausted retries or results permanently rejected.
- `judge.rejudge`: explicit rejudge tasks if separated from normal queue.

The Rust API also declares `judge.rejudge` and `judge.results` to preserve the
reviewed cross-service topology. Submission Outbox rows use expiring PostgreSQL
leases and stable judgement UUID message IDs; therefore delivery is at least
once. The Rust result consumer deduplicates by immutable result message UUID in
the same transaction that persists the judgement, runs, submission status, and
TEAM event. It ACKs only after commit; invalid/conflicting messages dead-letter
and transient database failures requeue.

A dedicated dead-letter consumer reads `judge.dead`. Each message (a
dead-lettered `JudgeTask` or a permanently rejected `JudgeResult`) carries the
judgement and submission it belongs to; the consumer atomically marks a
submission that is genuinely stuck in `judging`/`pending` as `system_error`
(with an audit row and a realtime TEAM event) and acknowledges the message. A
judgement that is already completed or superseded is left untouched, so recovery
is idempotent. This guarantees a dead-lettered task never leaves a submission
reporting "judging" forever.

Workers must ACK after the system has either persisted a final result or safely moved the task to a retry/dead-letter path. Unhandled worker process exits should not silently lose tasks.

## Judgement States

Submission states:

- `pending`
- `judging`
- `accepted`
- `wrong_answer`
- `time_limit_exceeded`
- `memory_limit_exceeded`
- `runtime_error`
- `compile_error`
- `output_limit_exceeded`
- `system_error`
- `cancelled`

Internal run records should store per-test-case status, CPU time, wall time, memory, exit code, signal, and output hash or truncated output reference.

## Sandbox Requirements

The sandbox must enforce:

- No network access.
- CPU time limit.
- Memory limit.
- Process count limit.
- File size limit.
- Output size limit.
- Isolated filesystem.
- Read-only test data mount.
- Temporary directory cleanup after every run.

**Production / contest deployments**: Judge containers must not mount the Docker socket and must not run as privileged containers.

**Development sandbox exception**: For local development and CI, the worker may run as a sibling-docker container (mounting `/var/run/docker.sock`) provided that (a) the worker process runs as a non-root user added to the `docker` group, (b) the worker never constructs shell or Docker CLI command strings from user-controlled input and uses only typed Docker API parameters through the Rust `bollard` client, and (c) every sandbox container is created with no network, a read-only root filesystem, `no-new-privileges`, a PID limit, a non-root UID/GID, and automatic cleanup. The threat model: user-submitted code is the untrusted boundary and stays inside the sandbox container; worker code is part of the trusted base.

### Sandbox Implementation Variants

Production options:

- nsjail.
- bubblewrap.
- runsc (gVisor).
- non-privileged Firecracker.

Development option:

- sibling-docker, with the development sandbox exception constraints described above.

## Language Support

Supported judge languages:

- C
- C++
- Java
- Go
- Rust
- Python

(`output` is the OUTPUT_ONLY pseudo-language that scores uploaded archives
without executing contestant code.)

### Current Rust Worker Slice

The Worker now consumes `judge.tasks` with bounded prefetch and validates both
the JSON contract and the AMQP message ID. It verifies the existing topology
passively, so startup fails instead of silently creating a divergent queue.
Malformed or permanently invalid tasks are Publisher-Confirmed to `judge.dead`
before the original task is ACKed. Transient storage/sandbox failures reject to
the task retry/TTL path. A completed result is published persistently to
`judge.results` and the task is ACKed only after Publisher Confirm. The result
message UUID is the judgement UUID, keeping replay after a Worker crash
idempotent at the API consumer.

RustFS source and test-data downloads are size bounded and SHA-256 verified.
Test-data cache names include problem, immutable version, and hash; corrupted
cache entries are discarded. Per-judgement work directories are private and
removed after every outcome.

All six source languages compile or syntax-check and execute through Bollard
against fixed `judge-runtime-c/cpp:12.2.0`, `judge-runtime-java:21`,
`judge-runtime-python:3.12.13`, `judge-runtime-go:1.24`, and
`judge-runtime-rust:1.88` images. Java receives a 2× time multiplier and
Python 3×; C, C++, Go, and Rust remain 1×. Go compiles a single `main.go`
file with its build caches on the read-write `/work` mount and a serial
(`GOMAXPROCS=1`) build so the sandbox PID limit is respected; Rust compiles a
single `main.rs` file with `rustc --edition 2021 -O` against the standard
library only. Each judgement creates one container, compiles once,
and executes its cases sequentially through Docker exec before forced removal.
The compile phase starts with a 1 GiB allowance; after successful compilation
the container cgroup is tightened to the problem memory limit. Containers have
no network, read-only root
filesystems, all capabilities dropped, `no-new-privileges`, non-root identity,
PID/CPU/memory limits and bounded output. Canonical answers are never mounted
into the contestant container: the Worker copies only the current input into
the work directory and compares output against answers kept on the Worker side.
Verdicts cover compile error, wrong answer, time, memory,
runtime and output limits. Standard comparison normalizes CR/LF, trailing
spaces/tabs per line, and trailing blank lines, but otherwise compares bytes
exactly. Workers publish confirmed, versioned heartbeats through RabbitMQ with process instance,
capacity, active task count, supported languages, runtime image versions, and sandbox runtime.
The API persists those heartbeats in PostgreSQL and treats a Worker as online for 15 seconds after
its latest report. `JUDGE_TASK_PREFETCH` defines the real parallel execution capacity; shutdown
stops new deliveries and drains in-flight work before closing the RabbitMQ connection.

Runtime images are built from `deploy/judge/runtimes` and include GNU `time`.
The Worker charges each run's user + system CPU time against the
language-adjusted limit and records GNU `time` peak RSS. The metrics record is
emitted by the timing parent after the submitted process exits, extracted from
the end of container stderr, and removed from the contestant-visible log. A
three-times wall-clock deadline remains the deadlock/sleep safety boundary;
Docker cgroup sampling remains a fallback for containers killed before GNU
`time` can report. Build the fixed local tags with
`scripts/build-judge-runtimes.sh`. Production `runsc` acceptance remains
follow-up work.

Task acknowledgement remains strictly after a confirmed result publication. If RabbitMQ restarts
after a Worker has accepted a task but before that boundary, the durable unacknowledged delivery is
requeued. The Worker reconnects and evaluates it again under at-least-once semantics; the stable
judgement/message identifier lets the API result transaction collapse any later duplicate.

Each language runtime should define compile command, run command, file extension, source size limit, timeout multiplier if needed, and version string exposed in admin diagnostics.

## Test Data Handling

RustFS is the source of truth for uploaded test data. Workers may maintain a local cache.

Cache rules:

- Cache keys should include problem ID, data version, and hash.
- Workers must verify hash before judging.
- Stale cache entries must not be used after problem data changes.
- Official contest freeze should lock test data versions.

## Failure Handling

Expected failures and required behavior:

| Failure | Behavior |
|---|---|
| Worker exits during task | RabbitMQ redelivers or task is retried |
| Compile tool missing | Mark `system_error` and raise health alert |
| Sandbox setup failed | Mark `system_error` and raise health alert |
| RustFS read failed | Retry if transient; dead-letter if repeated |
| Result write failed | Retry before ACK |
| Test data hash mismatch | Stop judging and mark `system_error` |

## Observability

Workers and scheduler should expose:

- Worker online count.
- Judge slot capacity and usage.
- Queue depth.
- Task latency.
- Compile error rate.
- System error rate.
- Average and p95 judge duration.
- Per-language submission count.
- Local test data cache hit ratio.
