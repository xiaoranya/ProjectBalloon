#!/usr/bin/env bash
set -euo pipefail

REPORT_DIR="${REPORT_DIR:?Set REPORT_DIR to the rehearsal evidence directory}"
CONTEST_ID="${CONTEST_ID:?Set CONTEST_ID to the rehearsal contest ID}"
SAMPLE_SECONDS="${SAMPLE_SECONDS:-30}"
POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-project-balloon-it-postgres}"
REDIS_CONTAINER="${REDIS_CONTAINER:-project-balloon-it-redis}"
RABBITMQ_CONTAINER="${RABBITMQ_CONTAINER:-project-balloon-it-rabbitmq}"
RABBITMQ_VHOST="${RABBITMQ_VHOST:-project_balloon}"
STOP_FILE="$REPORT_DIR/monitor.stop"

mkdir -p "$REPORT_DIR"
rm -f "$STOP_FILE"

API_PID="${API_PID:-$(pgrep -n -f '^target/(debug|release)/project-balloon-api$')}"
WORKER_CONTAINERS="${WORKER_CONTAINERS:-}"
if [ -n "$WORKER_CONTAINERS" ]; then
  IFS=',' read -r -a worker_containers <<< "$WORKER_CONTAINERS"
  worker_pids=()
  for container in "${worker_containers[@]}"; do
    worker_pids+=("$(docker inspect --format '{{.State.Pid}}' "$container")")
  done
  WORKER_PID="$(IFS=,; echo "${worker_pids[*]}")"
else
  WORKER_PID="${WORKER_PID:-$(pgrep -n -f '^target/(debug|release)/project-balloon-judge-worker$')}"
fi

{
  date -u '+captured_at=%Y-%m-%dT%H:%M:%SZ'
  uname -a
  lscpu
  free -h
  df -h / /tmp "$REPORT_DIR"
  rustc --version
  cargo --version
  docker version --format 'docker_client={{.Client.Version}} docker_server={{.Server.Version}}'
  docker images --format '{{.Repository}}:{{.Tag}} {{.ID}} {{.Size}}' | sort
  printf 'api_pid=%s\nworker_pids=%s\nworker_containers=%s\n' \
    "$API_PID" "$WORKER_PID" "$WORKER_CONTAINERS"
} > "$REPORT_DIR/hardware.txt"

printf 'timestamp,api_cpu_percent,api_rss_kb,api_vsz_kb,worker_cpu_percent,worker_rss_kb,worker_vsz_kb\n' > "$REPORT_DIR/process.csv"
printf 'timestamp,connections,active,xact_commit,blks_hit,blks_read,temp_bytes,deadlocks,active_over_500ms\n' > "$REPORT_DIR/postgres.csv"
printf 'timestamp,keyspace_hits,keyspace_misses,instantaneous_ops_per_sec,used_memory_bytes\n' > "$REPORT_DIR/redis.csv"
printf 'timestamp,messages_ready,messages_unacknowledged,consumers\n' > "$REPORT_DIR/rabbitmq.csv"
printf 'timestamp,submissions,completed,pending,system_errors,avg_queue_wait_seconds,avg_judge_seconds\n' > "$REPORT_DIR/judge.csv"

sample() {
  local timestamp api_stats worker_stats pg_stats redis_stats rabbit_stats judge_stats
  timestamp="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  api_stats="$(ps -p "$API_PID" -o %cpu=,rss=,vsz= | xargs | tr ' ' ',')"
  worker_stats="$(ps -p "$WORKER_PID" -o %cpu=,rss=,vsz= | awk \
    '{cpu+=$1;rss+=$2;vsz+=$3} END{printf "%.1f,%d,%d",cpu,rss,vsz}')"
  printf '%s,%s,%s\n' "$timestamp" "$api_stats" "$worker_stats" >> "$REPORT_DIR/process.csv"

  pg_stats="$(docker exec "$POSTGRES_CONTAINER" psql -U project_balloon -d project_balloon -At -F, -c "SELECT numbackends,(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND state='active'),xact_commit,blks_hit,blks_read,temp_bytes,deadlocks,(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND state='active' AND now()-query_start>interval '500 milliseconds') FROM pg_stat_database WHERE datname=current_database();")"
  printf '%s,%s\n' "$timestamp" "$pg_stats" >> "$REPORT_DIR/postgres.csv"

  redis_stats="$(docker exec "$REDIS_CONTAINER" redis-cli INFO | tr -d '\r' | awk -F: '/^(keyspace_hits|keyspace_misses|instantaneous_ops_per_sec|used_memory):/{value[$1]=$2} END{printf "%s,%s,%s,%s",value["keyspace_hits"],value["keyspace_misses"],value["instantaneous_ops_per_sec"],value["used_memory"]}')"
  printf '%s,%s\n' "$timestamp" "$redis_stats" >> "$REPORT_DIR/redis.csv"

  rabbit_stats="$(docker exec "$RABBITMQ_CONTAINER" rabbitmqctl -q list_queues -p "$RABBITMQ_VHOST" name messages_ready messages_unacknowledged consumers | awk '$1=="judge.tasks"{printf "%s,%s,%s",$2,$3,$4}')"
  printf '%s,%s\n' "$timestamp" "$rabbit_stats" >> "$REPORT_DIR/rabbitmq.csv"

  judge_stats="$(docker exec "$POSTGRES_CONTAINER" psql -U project_balloon -d project_balloon -At -F, -c "SELECT count(*),count(*) FILTER(WHERE j.completed_at IS NOT NULL),count(*) FILTER(WHERE j.completed_at IS NULL),count(*) FILTER(WHERE j.verdict='SYSTEM_ERROR'),coalesce(round(avg(extract(epoch FROM j.started_at-s.created_at))::numeric,6),0),coalesce(round(avg(extract(epoch FROM j.completed_at-j.started_at))::numeric,6),0) FROM submissions s LEFT JOIN judgements j ON j.submission_id=s.id WHERE s.contest_id=$CONTEST_ID;")"
  printf '%s,%s\n' "$timestamp" "$judge_stats" >> "$REPORT_DIR/judge.csv"
}

while [ ! -e "$STOP_FILE" ]; do
  sample
  sleep "$SAMPLE_SECONDS"
done
