#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGES="${1:-$ROOT/images}"
[ -f "$IMAGES/SHA256SUMS" ] || { echo "missing $IMAGES/SHA256SUMS" >&2; exit 1; }
(cd "$IMAGES" && sha256sum -c SHA256SUMS)
found=0
for archive in "$IMAGES"/*.tar; do
  [ -f "$archive" ] || continue
  found=1
  docker load --input "$archive"
done
[ "$found" -eq 1 ] || { echo "no image archives found in $IMAGES" >&2; exit 1; }
