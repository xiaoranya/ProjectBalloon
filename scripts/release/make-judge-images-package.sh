#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
IMAGES="${1:-$ROOT/judge-images}"
OUT_BASE="${2:-$ROOT/dist}"

[ -f "$IMAGES/SHA256SUMS" ] || { echo 'export judge images before packaging' >&2; exit 1; }
(cd "$IMAGES" && sha256sum -c SHA256SUMS)
PACKAGE_TARGET="${PROJECT_BALLOON_PACKAGE_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
[ -n "$PACKAGE_TARGET" ] || { echo 'could not determine package target' >&2; exit 1; }

FINAL="$OUT_BASE/project-balloon-$VERSION-$PACKAGE_TARGET-judge-images.tar.gz"
STAGING="$OUT_BASE/.judge-images-staging"
[ ! -e "$FINAL" ] || { echo "output already exists: $FINAL" >&2; exit 1; }

cleanup() {
  local status="$?"
  if [ "$status" -ne 0 ]; then
    rm -rf "$STAGING" "$FINAL.tmp"
  fi
}
trap cleanup EXIT

rm -rf "$STAGING"
mkdir -p "$STAGING/judge-images"
cp -a "$IMAGES/." "$STAGING/judge-images/"
tar -C "$STAGING" -czf "$FINAL.tmp" judge-images
tar -tzf "$FINAL.tmp" >/dev/null
(cd "$STAGING/judge-images" && sha256sum -c SHA256SUMS)
mkdir -p "$OUT_BASE"
mv "$FINAL.tmp" "$FINAL"
rm -rf "$STAGING"
printf '[release] judge images archive ready: %s\n' "$FINAL"
