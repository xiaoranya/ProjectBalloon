#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
IMAGES="${1:-$ROOT/images}"
OUT_BASE="${2:-$ROOT/dist}"
FINAL="$OUT_BASE/project-balloon-offline-$VERSION"
TMP="$FINAL.tmp"

[ -f "$IMAGES/SHA256SUMS" ] || { echo 'export images before packaging' >&2; exit 1; }
(cd "$IMAGES" && sha256sum -c SHA256SUMS)
[ ! -e "$FINAL" ] && [ ! -e "$TMP" ] || { echo "output already exists: $FINAL" >&2; exit 1; }
mkdir -p "$TMP/images"
cleanup() {
  local status="$?"
  if [ "$status" -ne 0 ]; then
    rm -rf "$TMP" "$TMP.PACKAGE-SHA256SUMS"
  fi
}
trap cleanup EXIT
cp "$IMAGES"/*.tar "$IMAGES/SHA256SUMS" "$TMP/images/"
tar -C "$ROOT" \
  --exclude='deploy/compose/.env.rust' --exclude='deploy/compose/.env.local' \
  --exclude='frontend/web/node_modules' --exclude='frontend/web/dist' \
  -cf - VERSION deploy docs scripts | tar -C "$TMP" -xf -
cat > "$TMP/README.txt" <<EOF
ProjectBalloon offline package $VERSION

Prerequisites: Docker Engine with Compose plugin, curl, gzip, and AWS CLI v2.
Run:
  scripts/deploy/load-images.sh
  scripts/deploy/install.sh
  # edit deploy/compose/.env.rust
  scripts/deploy/install.sh
  scripts/deploy/start.sh all
  scripts/deploy/healthcheck.sh all
EOF
(cd "$TMP" && find . -type f ! -name PACKAGE-SHA256SUMS -print0 | sort -z | xargs -0 sha256sum) > "$TMP.PACKAGE-SHA256SUMS"
mv "$TMP.PACKAGE-SHA256SUMS" "$TMP/PACKAGE-SHA256SUMS"
mkdir -p "$OUT_BASE"
mv "$TMP" "$FINAL"
printf '[package] complete: %s\n' "$FINAL"
