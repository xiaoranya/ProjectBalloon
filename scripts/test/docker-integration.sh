#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ID="${PROJECT_BALLOON_IT_RUN_ID:-$$}"
PREFIX="project-balloon-it-$RUN_ID"
POSTGRES_CONTAINER="$PREFIX-postgres"
REDIS_CONTAINER="$PREFIX-redis"
RABBIT_CONTAINER="$PREFIX-rabbitmq"
RUSTFS_CONTAINER="$PREFIX-rustfs"
API_PID=""
DIAGNOSTICS_DIR="${PROJECT_BALLOON_IT_DIAGNOSTICS_DIR:-}"

log() { printf '[docker-it] %s\n' "$*"; }
die() { printf '[docker-it] ERROR: %s\n' "$*" >&2; exit 1; }

collect_diagnostics() {
  [ -n "$DIAGNOSTICS_DIR" ] || return 0
  mkdir -p "$DIAGNOSTICS_DIR"
  for container in "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" \
    "$RABBIT_CONTAINER" "$RUSTFS_CONTAINER"; do
    docker inspect "$container" >"$DIAGNOSTICS_DIR/$container.inspect.json" 2>&1 || true
    docker logs "$container" >"$DIAGNOSTICS_DIR/$container.log" 2>&1 || true
  done
}

pick_port() {
  local candidate
  for _ in $(seq 1 200); do
    candidate=$((20000 + RANDOM))
    if ! ss -H -ltn "sport = :$candidate" | grep -q .; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  die 'could not select a free local TCP port'
}

assert_port_free() {
  if ss -H -ltn "sport = :$1" | grep -q .; then
    die "local TCP port $1 is already in use"
  fi
}

cleanup() {
  local status="$?"
  if [ -n "$API_PID" ] && kill -0 "$API_PID" 2>/dev/null; then
    kill -INT "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
  if [ "$status" -ne 0 ]; then
    collect_diagnostics
  fi
  docker rm -f "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" \
    "$RABBIT_CONTAINER" "$RUSTFS_CONTAINER" >/dev/null 2>&1 || true
  if [ "$status" -ne 0 ]; then
    log "failed with exit=$status; temporary containers removed"
  fi
}
trap cleanup EXIT INT TERM

command -v docker >/dev/null 2>&1 || die 'docker is required'
command -v cargo >/dev/null 2>&1 || die 'cargo is required'
command -v aws >/dev/null 2>&1 || die 'AWS CLI v2 is required'
command -v curl >/dev/null 2>&1 || die 'curl is required'
command -v ss >/dev/null 2>&1 || die 'iproute2 ss is required for collision-free test ports'
docker info >/dev/null 2>&1 || die 'Docker daemon is unavailable'

POSTGRES_PORT="${PROJECT_BALLOON_IT_POSTGRES_PORT:-$(pick_port)}"
REDIS_PORT="${PROJECT_BALLOON_IT_REDIS_PORT:-$(pick_port)}"
RABBIT_PORT="${PROJECT_BALLOON_IT_RABBIT_PORT:-$(pick_port)}"
RUSTFS_PORT="${PROJECT_BALLOON_IT_RUSTFS_PORT:-$(pick_port)}"
API_PORT="${PROJECT_BALLOON_IT_API_PORT:-$(pick_port)}"
for port in "$POSTGRES_PORT" "$REDIS_PORT" "$RABBIT_PORT" "$RUSTFS_PORT" "$API_PORT"; do
  assert_port_free "$port"
done
unique_ports="$(printf '%s\n' "$POSTGRES_PORT" "$REDIS_PORT" "$RABBIT_PORT" "$RUSTFS_PORT" "$API_PORT" | sort -u | wc -l)"
[ "$unique_ports" -eq 5 ] || die 'selected integration-test ports are not unique; retry the command'

cd "$ROOT"
log 'starting isolated PostgreSQL, Redis, RabbitMQ, and RustFS'
docker run -d --name "$POSTGRES_CONTAINER" \
  -e POSTGRES_DB=project_balloon_test \
  -e POSTGRES_USER=project_balloon_test \
  -e POSTGRES_PASSWORD=test-postgres-password \
  -p "127.0.0.1:$POSTGRES_PORT:5432" postgres:16.6 >/dev/null
docker run -d --name "$REDIS_CONTAINER" \
  -p "127.0.0.1:$REDIS_PORT:6379" redis:7.4.2 \
  redis-server --appendonly no >/dev/null
docker run -d --name "$RABBIT_CONTAINER" \
  -e RABBITMQ_DEFAULT_USER=test -e RABBITMQ_DEFAULT_PASS=test-rabbit \
  -e RABBITMQ_DEFAULT_VHOST=test \
  -e RABBITMQ_ERLANG_COOKIE=project_balloon_integration_cookie \
  -p "127.0.0.1:$RABBIT_PORT:5672" \
  rabbitmq:3.13.7-management >/dev/null
docker run -d --name "$RUSTFS_CONTAINER" \
  -e RUSTFS_ACCESS_KEY=test -e RUSTFS_SECRET_KEY=test-rustfs \
  -p "127.0.0.1:$RUSTFS_PORT:9000" rustfs/rustfs:1.0.0-beta.8 >/dev/null

log 'waiting for dependencies'
ready=0
for _ in $(seq 1 60); do
  if docker exec "$POSTGRES_CONTAINER" pg_isready -U project_balloon_test \
      -d project_balloon_test >/dev/null 2>&1 \
    && docker exec "$REDIS_CONTAINER" redis-cli ping 2>/dev/null | grep -q PONG \
    && docker exec "$RABBIT_CONTAINER" rabbitmq-diagnostics -q ping >/dev/null 2>&1 \
    && AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test-rustfs \
      aws --endpoint-url "http://127.0.0.1:$RUSTFS_PORT" s3api list-buckets >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 2
done
[ "$ready" -eq 1 ] || die 'dependencies did not become ready within 120 seconds'

export DATABASE_URL="postgresql://project_balloon_test:test-postgres-password@127.0.0.1:$POSTGRES_PORT/project_balloon_test"
export PROJECT_BALLOON_TEST_REDIS_URL="redis://127.0.0.1:$REDIS_PORT"
export PROJECT_BALLOON_TEST_REDIS_CONTAINER="$REDIS_CONTAINER"
export PROJECT_BALLOON_TEST_AMQP_URL="amqp://test:test-rabbit@127.0.0.1:$RABBIT_PORT/test"
export PROJECT_BALLOON_TEST_RABBITMQ_CONTAINER="$RABBIT_CONTAINER"
export PROJECT_BALLOON_TEST_S3_ENDPOINT="http://127.0.0.1:$RUSTFS_PORT"
export PROJECT_BALLOON_TEST_S3_REGION=us-east-1
export PROJECT_BALLOON_TEST_S3_ACCESS_KEY=test
export PROJECT_BALLOON_TEST_S3_SECRET_KEY=test-rustfs
export PROJECT_BALLOON_TEST_S3_BUCKET=project-balloon-integration
export PROJECT_BALLOON_TEST_S3_PROBLEM_BUCKET=xcpc-problems
export PROJECT_BALLOON_TEST_S3_SOURCE_BUCKET=xcpc-sources
export AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test-rustfs

for bucket in project-balloon-integration xcpc-problems xcpc-sources; do
  aws --endpoint-url "$PROJECT_BALLOON_TEST_S3_ENDPOINT" s3 mb "s3://$bucket" >/dev/null
done

log 'running API/PostgreSQL/Redis/RustFS integration tests'
cargo test -p project-balloon-api --all-features --quiet -- --ignored --test-threads=1

log 'starting API briefly to declare RabbitMQ topology'
DATABASE_URL="$DATABASE_URL" PROJECT_BALLOON_API_BIND="127.0.0.1:$API_PORT" \
PROJECT_BALLOON_RUN_MIGRATIONS=true PROJECT_BALLOON_SECURE_COOKIES=false \
PROJECT_BALLOON_CSRF_SECRET=test-csrf-secret-012345678901234567890123 \
PROJECT_BALLOON_REALTIME_REDIS_ENABLED=true REDIS_URL="$PROJECT_BALLOON_TEST_REDIS_URL" \
PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED=false PROJECT_BALLOON_OBJECT_STORAGE_ENABLED=true \
PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT="$PROJECT_BALLOON_TEST_S3_ENDPOINT" \
PROJECT_BALLOON_OBJECT_STORAGE_REGION=us-east-1 PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY=test \
PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY=test-rustfs PROJECT_BALLOON_RABBITMQ_ENABLED=true \
PROJECT_BALLOON_RABBITMQ_URL="$PROJECT_BALLOON_TEST_AMQP_URL" \
  cargo run -p project-balloon-api --bin project-balloon-api >/dev/null &
API_PID="$!"
for _ in $(seq 1 60); do
  if curl --fail --silent "http://127.0.0.1:$API_PORT/livez" >/dev/null 2>&1; then break; fi
  kill -0 "$API_PID" 2>/dev/null || die 'API exited before topology initialization'
  sleep 1
done
curl --fail --silent "http://127.0.0.1:$API_PORT/livez" >/dev/null \
  || die 'API did not become live within 60 seconds'
curl --fail --silent "http://127.0.0.1:$API_PORT/api/openapi.json" >/dev/null \
  || die 'API OpenAPI document is not available'
health_status=000
for _ in $(seq 1 30); do
  health_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
    "http://127.0.0.1:$API_PORT/api/health")"
  [ "$health_status" = 200 ] && break
  sleep 1
done
[ "$health_status" = 200 ] || die "API readiness returned HTTP $health_status"
for protected_path in /api/problems /api/problems/1 /api/contests/1/problems; do
  status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
    "http://127.0.0.1:$API_PORT$protected_path")"
  case "$status" in
    401|403) ;;
    *) die "unauthenticated $protected_path returned HTTP $status" ;;
  esac
done
kill -INT "$API_PID"; wait "$API_PID"; API_PID=""

purge_judge_queues() {
  docker exec "$RABBIT_CONTAINER" rabbitmqctl purge_queue judge.tasks -p test >/dev/null
  docker exec "$RABBIT_CONTAINER" rabbitmqctl purge_queue judge.results -p test >/dev/null
}

log 'running RabbitMQ/RustFS/C++ end-to-end pipeline'
cargo test -p project-balloon-judge-worker --test pipeline_docker \
  rabbit_rustfs_cpp_pipeline_publishes_confirmed_result -- --ignored --exact --test-threads=1
purge_judge_queues
log 'running Worker concurrency and graceful-drain test'
cargo test -p project-balloon-judge-worker --test rabbit_concurrency \
  capacity_two_runs_in_parallel_and_shutdown_drains_in_flight_tasks \
  -- --ignored --exact --test-threads=1
purge_judge_queues
log 'running RabbitMQ restart recovery test'
cargo test -p project-balloon-judge-worker --test rabbit_concurrency \
  broker_restart_requeues_unacknowledged_in_flight_task -- --ignored --exact --test-threads=1
log 'running real Docker sandbox tests'
cargo test -p project-balloon-judge-worker --test sandbox_docker -- --ignored --test-threads=1
log 'all Docker-backed integration tests passed'
