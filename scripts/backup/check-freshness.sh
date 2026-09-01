#!/usr/bin/env bash
# Exit non-zero when the newest ProjectBalloon backup is older than
# BACKUP_MAX_AGE_HOURS (default 26, i.e. one missed daily run plus slack), so
# cron, a systemd timer, or an external monitor can raise an operator alert.
# Usage: check-freshness.sh [backup-directory]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$ROOT/scripts/lib/deploy-common.sh"

BACKUP_DIR="${1:-/var/backups/project-balloon}"
MAX_AGE_HOURS="${BACKUP_MAX_AGE_HOURS:-26}"

[ -d "$BACKUP_DIR" ] || pb_die "backup directory does not exist: $BACKUP_DIR"
latest="$(find "$BACKUP_DIR" -maxdepth 1 -type d -name 'project-balloon-*' ! -name '*.tmp' | sort | tail -n1)"
[ -n "$latest" ] || pb_die "no completed backup found under $BACKUP_DIR"

now="$(date -u +%s)"
stamp="$(basename "$latest")"
stamp="${stamp#project-balloon-}"
case "$stamp" in
  [0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]Z)
    created="$(date -u -d "${stamp:0:4}-${stamp:4:2}-${stamp:6:2} ${stamp:9:2}:${stamp:11:2}:${stamp:13:2}" +%s)"
    ;;
  *)
    # Unrecognized name; fall back to the directory mtime.
    created="$(stat -c %Y "$latest")"
    ;;
esac
age_hours=$(( (now - created) / 3600 ))
if [ "$age_hours" -gt "$MAX_AGE_HOURS" ]; then
  pb_die "backup is stale: latest=$latest age=${age_hours}h exceeds BACKUP_MAX_AGE_HOURS=${MAX_AGE_HOURS}h"
fi
pb_log "backup is fresh: latest=$latest age=${age_hours}h (limit ${MAX_AGE_HOURS}h)"
