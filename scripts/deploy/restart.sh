#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROLE="${1:-all}"

"$SCRIPT_DIR/stop.sh" "$ROLE"
"$SCRIPT_DIR/start.sh" "$ROLE"
