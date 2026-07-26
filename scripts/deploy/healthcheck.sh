#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/deploy-common.sh
. "$SCRIPT_DIR/../lib/deploy-common.sh"
# shellcheck source=../lib/env-file.sh
. "$SCRIPT_DIR/../lib/env-file.sh"

ROLE="${1:-all}"
pb_require_docker
pb_require_env
pb_load_env_file "$PB_ENV_FILE" WEB_PORT
TIMEOUT_SECONDS="${PROJECT_BALLOON_HEALTH_TIMEOUT_SECONDS:-120}"
POLL_SECONDS="${PROJECT_BALLOON_HEALTH_POLL_SECONDS:-2}"
API_URL="${PROJECT_BALLOON_HEALTH_URL:-http://127.0.0.1:${WEB_PORT:-8088}/api/health}"

check_role() {
  local role="$1" deadline lines bad starting
  deadline=$((SECONDS + TIMEOUT_SECONDS))
  while true; do
    lines="$(pb_compose "$role" ps --format '{{.State}} {{.Health}}')"
    [ -n "$lines" ] || pb_die "role=$role has no running services"
    bad="$(printf '%s\n' "$lines" | awk 'NF && ($1 != "running" || $2 == "unhealthy")' | wc -l)"
    starting="$(printf '%s\n' "$lines" | awk '$2 == "starting"' | wc -l)"
    [ "$bad" -eq 0 ] || pb_die "role=$role has $bad unhealthy or stopped service(s)"
    if [ "$starting" -eq 0 ]; then
      pb_log "role=$role containers are ready"
      return
    fi
    [ "$SECONDS" -lt "$deadline" ] || pb_die "role=$role health check timed out"
    sleep "$POLL_SECONDS"
  done
}

while IFS= read -r role; do
  check_role "$role"
done < <(pb_roles_forward "$ROLE")

if [ "$ROLE" = all ] || [ "$ROLE" = app ]; then
  command -v curl >/dev/null 2>&1 || pb_die 'curl is required for API readiness checks'
  curl --fail --silent --show-error "$API_URL" >/dev/null || pb_die "API is not ready at $API_URL"
  pb_log "API readiness passed at $API_URL"
fi
