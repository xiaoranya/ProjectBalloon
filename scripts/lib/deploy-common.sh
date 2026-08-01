#!/usr/bin/env bash

# Shared helpers for the Rust deployment scripts. This file is sourced; callers
# are expected to enable their own strict shell options.

PB_SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PB_ROOT="$(cd "$PB_SCRIPT_DIR/../.." && pwd)"
PB_COMPOSE_DIR="$PB_ROOT/deploy/compose"
if [ -n "${PROJECT_BALLOON_ENV_FILE:-}" ]; then
  PB_ENV_FILE="$PROJECT_BALLOON_ENV_FILE"
elif [ -f "$PB_COMPOSE_DIR/.env.rust" ]; then
  PB_ENV_FILE="$PB_COMPOSE_DIR/.env.rust"
else
  PB_ENV_FILE="/etc/project-balloon/project-balloon.env"
fi

pb_die() {
  printf '[deploy] ERROR: %s\n' "$*" >&2
  exit 1
}

pb_log() {
  printf '[deploy] %s\n' "$*"
}

pb_require_docker() {
  command -v docker >/dev/null 2>&1 || pb_die 'docker is not installed'
  docker compose version >/dev/null 2>&1 || pb_die 'docker compose plugin is not installed'
}

pb_require_env() {
  [ -f "$PB_ENV_FILE" ] || pb_die "missing $PB_ENV_FILE; run scripts/deploy/install.sh"
}

pb_compose_file() {
  case "$1" in
    data) printf '%s\n' "$PB_COMPOSE_DIR/data.docker-compose.yml" ;;
    app) printf '%s\n' "$PB_COMPOSE_DIR/rust-app.docker-compose.yml" ;;
    monitor) printf '%s\n' "$PB_ROOT/deploy/observability/compose.yml" ;;
    *) pb_die "unknown role '$1' (expected data, app, monitor, or all)" ;;
  esac
}

pb_roles_forward() {
  if [ "$1" = all ]; then
    printf '%s\n' data app monitor
  else
    pb_compose_file "$1" >/dev/null
    printf '%s\n' "$1"
  fi
}

pb_roles_reverse() {
  if [ "$1" = all ]; then
    printf '%s\n' monitor app data
  else
    pb_compose_file "$1" >/dev/null
    printf '%s\n' "$1"
  fi
}

pb_compose() {
  local role="$1"
  shift
  docker compose --env-file "$PB_ENV_FILE" -f "$(pb_compose_file "$role")" "$@"
}
