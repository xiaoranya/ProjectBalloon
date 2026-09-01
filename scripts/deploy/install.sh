#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/deploy-common.sh
. "$SCRIPT_DIR/../lib/deploy-common.sh"
# shellcheck source=../lib/env-file.sh
. "$SCRIPT_DIR/../lib/env-file.sh"

ROLE="${1:-all}"
EXAMPLE="$PB_COMPOSE_DIR/.env.rust.example"

pb_require_docker

if [ ! -f "$PB_ENV_FILE" ]; then
  [ -f "$EXAMPLE" ] || pb_die "missing environment template: $EXAMPLE"
  cp "$EXAMPLE" "$PB_ENV_FILE"
  chmod 0600 "$PB_ENV_FILE"
  pb_log "created $PB_ENV_FILE"
  pb_log 'edit every CHANGE_ME value, then run this command again'
  exit 2
fi

# The environment file contains database, broker, object-storage, and CSRF
# credentials. Re-assert the private mode on every validation run because an
# editor, copy, or restore can broaden it after the initial installation.
chmod 0600 "$PB_ENV_FILE"

if grep -Eq '^[A-Za-z_][A-Za-z0-9_]*=.*CHANGE_ME' "$PB_ENV_FILE"; then
  pb_die "$PB_ENV_FILE still contains CHANGE_ME values"
fi

# The judge cache directory is a host bind mount into the worker container,
# which runs as XCPC_SANDBOX_USER (default 1000:1000). A root-owned
# auto-created bind mount would make the worker's cache writes fail with
# EACCES, so create the directory here and assert its ownership.
JUDGE_CACHE_DIR="${JUDGE_CACHE_DIR:-/var/lib/project-balloon/judge-cache}"
case "$JUDGE_CACHE_DIR" in
  /*) ;;
  *) pb_die "JUDGE_CACHE_DIR must be an absolute path (got '$JUDGE_CACHE_DIR')" ;;
esac
sandbox_user="${XCPC_SANDBOX_USER:-1000:1000}"
cache_uid="${sandbox_user%%:*}"
cache_gid="$cache_uid"
[[ "$sandbox_user" == *:* ]] && cache_gid="${sandbox_user#*:}"
mkdir -p "$JUDGE_CACHE_DIR"
cache_owner="$(stat -c '%u:%g' "$JUDGE_CACHE_DIR")"
if [ "$cache_owner" != "$cache_uid:$cache_gid" ]; then
  if [ "$(id -u)" -eq 0 ]; then
    chown "$cache_uid:$cache_gid" "$JUDGE_CACHE_DIR"
    pb_log "fixed judge cache ownership: $JUDGE_CACHE_DIR -> $cache_uid:$cache_gid"
  else
    pb_die "judge cache directory $JUDGE_CACHE_DIR is owned by $cache_owner but the worker container runs as $cache_uid:$cache_gid; run: sudo chown $cache_uid:$cache_gid '$JUDGE_CACHE_DIR'"
  fi
fi
pb_log "judge cache directory ready: $JUDGE_CACHE_DIR ($cache_uid:$cache_gid)"

pb_load_env_file "$PB_ENV_FILE" XCPC_SANDBOX_RUNTIME JUDGE_C_IMAGE JUDGE_CACHE_DIR XCPC_SANDBOX_USER
if [ -n "${XCPC_SANDBOX_RUNTIME:-}" ]; then
  runtime_json="$(docker info --format '{{json .Runtimes}}' 2>/dev/null || true)"
  printf '%s' "$runtime_json" | grep -Fq "\"$XCPC_SANDBOX_RUNTIME\"" \
    || pb_die "configured sandbox runtime '$XCPC_SANDBOX_RUNTIME' is not registered with Docker"
  [ -n "${JUDGE_C_IMAGE:-}" ] || pb_die 'JUDGE_C_IMAGE is empty'
  docker run --rm --network none --runtime "$XCPC_SANDBOX_RUNTIME" \
    "$JUDGE_C_IMAGE" /usr/bin/time --version >/dev/null \
    || pb_die "sandbox runtime '$XCPC_SANDBOX_RUNTIME' cannot start $JUDGE_C_IMAGE"
  pb_log "sandbox runtime preflight passed: $XCPC_SANDBOX_RUNTIME"
fi

while IFS= read -r role; do
  pb_log "validating compose role=$role"
  pb_compose "$role" config --quiet
done < <(pb_roles_forward "$ROLE")

pb_log 'configuration validation complete'
