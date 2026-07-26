#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
PROFILE="${PROFILE:-smoke}"
ACCOUNTS_FILE="${ACCOUNTS_FILE:-}"
REPORT_DIR="${REPORT_DIR:-$ROOT_DIR/build/reports/k6}"
K6_IMAGE="${K6_IMAGE:-grafana/k6:0.57.0}"
SAVE_K6_JSON="${SAVE_K6_JSON:-false}"

case "$PROFILE" in
  smoke|rehearsal|full) ;;
  *) echo "PROFILE must be smoke, rehearsal, or full" >&2; exit 2 ;;
esac

case "$BASE_URL" in
  http://127.0.0.1:*|http://localhost:*|https://127.0.0.1:*|https://localhost:*) ;;
  *)
    if [ "${XCPC_PRESSURE_TARGET_ACK:-}" != "I_UNDERSTAND_THIS_GENERATES_LOAD" ]; then
      echo "Refusing to load-test non-local target $BASE_URL." >&2
      echo "Set XCPC_PRESSURE_TARGET_ACK=I_UNDERSTAND_THIS_GENERATES_LOAD after authorization." >&2
      exit 2
    fi
    ;;
esac

if [ -z "$ACCOUNTS_FILE" ] || [ ! -r "$ACCOUNTS_FILE" ]; then
  echo "Set ACCOUNTS_FILE to a readable, untracked account JSON file." >&2
  exit 2
fi

mkdir -p "$REPORT_DIR"
export BASE_URL PROFILE ACCOUNTS_FILE REPORT_DIR

if command -v k6 >/dev/null 2>&1; then
  k6_args=(run)
  if [ "$SAVE_K6_JSON" = "true" ]; then
    k6_args+=(--out "json=$REPORT_DIR/metrics.json")
  fi
  exec k6 "${k6_args[@]}" "$ROOT_DIR/scripts/pressure/k6/main.js"
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "Neither k6 nor Docker is available." >&2
  exit 127
fi

docker_env=(
  -e BASE_URL
  -e PROFILE
  -e CONTEST_ID
  -e ENABLE_WRITES
  -e ENABLE_SUBMISSIONS
  -e ENABLE_PRINTS
  -e RESOLVER_RUN_ID
  -e P95_LATENCY_MS
  -e MAX_FAILURE_RATE
  -e SUBMISSION_LANGUAGE
  -e SUBMISSION_SOURCE
  -e DURATION
  -e ACCOUNTS_FILE=/secure/accounts.json
  -e REPORT_DIR=/reports
)

docker_args=(run)
if [ "$SAVE_K6_JSON" = "true" ]; then
  docker_args+=(--out json=/reports/metrics.json)
fi

exec docker run --rm --network host \
  --user "$(id -u):$(id -g)" \
  "${docker_env[@]}" \
  -v "$ROOT_DIR:/work:ro" \
  -v "$ACCOUNTS_FILE:/secure/accounts.json:ro" \
  -v "$REPORT_DIR:/reports" \
  -w /work \
  "$K6_IMAGE" "${docker_args[@]}" scripts/pressure/k6/main.js
