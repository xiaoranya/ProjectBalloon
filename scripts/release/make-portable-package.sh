#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
STAGING="$ROOT/dist/binary-staging"
OUT_BASE="${1:-$ROOT/dist}"

[ -d "$STAGING/bin" ] || { echo 'build binary staging before packaging' >&2; exit 1; }
TARGET="${PROJECT_BALLOON_BINARY_TARGET:-$(sed -n 's/^PROJECT_BALLOON_TARGET=//p' "$STAGING/metadata/release.env")}"
PACKAGE_TARGET="${PROJECT_BALLOON_PACKAGE_TARGET:-$(sed -n 's/^PROJECT_BALLOON_PACKAGE_TARGET=//p' "$STAGING/metadata/release.env")}"
PACKAGE_TARGET="${PACKAGE_TARGET:-$TARGET}"
PLATFORM_TEST_STATUS="${PROJECT_BALLOON_PLATFORM_TEST_STATUS:-$(sed -n 's/^PROJECT_BALLOON_PLATFORM_TEST_STATUS=//p' "$STAGING/metadata/release.env")}"
PLATFORM_TEST_STATUS="${PLATFORM_TEST_STATUS:-linux-x86_64-only}"

[ -n "$TARGET" ] || { echo 'binary target is missing from staging metadata' >&2; exit 1; }
[ -n "$PACKAGE_TARGET" ] || { echo 'package target is missing from staging metadata' >&2; exit 1; }
FINAL="$OUT_BASE/project-balloon-$VERSION-$PACKAGE_TARGET"
TMP="$FINAL.tmp"
[ ! -e "$FINAL" ] && [ ! -e "$TMP" ] || { echo "output already exists: $FINAL" >&2; exit 1; }

cleanup() {
  local status="$?"
  if [ "$status" -ne 0 ]; then
    rm -rf "$TMP" "$TMP.PACKAGE-SHA256SUMS"
  fi
}
trap cleanup EXIT

mkdir -p "$TMP/bin" "$TMP/web" "$TMP/config" "$TMP/docs"
cp -a "$STAGING/bin/." "$TMP/bin/"
cp -a "$STAGING/web/." "$TMP/web/"
cp -a "$STAGING/metadata/." "$TMP/config/"
cp "$ROOT/LICENSE" "$TMP/LICENSE"
cp "$ROOT/docs/README.md" "$TMP/docs/README.md"

cat > "$TMP/README.txt" <<EOF
ProjectBalloon portable build $VERSION ($PACKAGE_TARGET)

Rust target: $TARGET
Platform validation: $PLATFORM_TEST_STATUS

This package contains the API, Judge Worker, bootstrap CLI, and Vue frontend
builds for the target platform. It is not a host installer and does not
include the Linux systemd/Nginx integration or Judge Runtime image archives.

Only Linux x86_64 has been tested end to end. Other platform packages are
build and package checks only until their runtime workflows have been validated
on the target host.
EOF

(cd "$TMP" && find . -type f ! -name PACKAGE-SHA256SUMS -print0 | sort -z \
  | xargs -0 sha256sum) > "$TMP.PACKAGE-SHA256SUMS"
mv "$TMP.PACKAGE-SHA256SUMS" "$TMP/PACKAGE-SHA256SUMS"
mkdir -p "$OUT_BASE"
mv "$TMP" "$FINAL"
printf '[release] portable package ready: %s\n' "$FINAL"
