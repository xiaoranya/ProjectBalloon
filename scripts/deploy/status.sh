#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/deploy-common.sh
. "$SCRIPT_DIR/../lib/deploy-common.sh"

ROLE="${1:-all}"
pb_require_docker
pb_require_env

while IFS= read -r role; do
  pb_log "status role=$role"
  pb_compose "$role" ps
done < <(pb_roles_forward "$ROLE")
