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
[ -n "$TARGET" ] || { echo 'binary target is missing from staging metadata' >&2; exit 1; }
[ -n "$PACKAGE_TARGET" ] || { echo 'package target is missing from staging metadata' >&2; exit 1; }
PLATFORM_TEST_STATUS="${PROJECT_BALLOON_PLATFORM_TEST_STATUS:-$(sed -n 's/^PROJECT_BALLOON_PLATFORM_TEST_STATUS=//p' "$STAGING/metadata/release.env")}"
PLATFORM_TEST_STATUS="${PLATFORM_TEST_STATUS:-linux-x86_64-only}"
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

mkdir -p "$TMP/bin" "$TMP/web" "$TMP/config" \
  "$TMP/systemd" "$TMP/nginx" "$TMP/lib" "$TMP/docs" "$TMP/scripts/backup" "$TMP/scripts/lib"
cp -a "$STAGING/bin/." "$TMP/bin/"
cp -a "$STAGING/web/." "$TMP/web/"
cp -a "$STAGING/metadata/." "$TMP/config/"
cp "$ROOT/install.sh" "$TMP/install.sh"
cp "$ROOT/deploy/binary/project-balloon.env.example" "$TMP/config/"
cp "$ROOT/deploy/binary/bootstrap-admin.env.example" "$TMP/config/"
cp "$ROOT/deploy/binary/project-balloon-api.service" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon-judge-worker.service" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon-backup.service" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon-backup.timer" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon.nginx.conf" "$TMP/nginx/"
cp "$ROOT/scripts/lib/env-file.sh" "$TMP/lib/"
cp "$ROOT/scripts/lib/deploy-common.sh" "$TMP/scripts/lib/"
cp "$ROOT/scripts/lib/env-file.sh" "$TMP/scripts/lib/"
cp -a "$ROOT/docs/ops/." "$TMP/docs/"
cp -a "$ROOT/scripts/backup/." "$TMP/scripts/backup/"
cp "$ROOT/VERSION" "$TMP/"

cat > "$TMP/README.txt" <<EOF
ProjectBalloon deployment release $VERSION ($PACKAGE_TARGET)

Rust target: $TARGET
Platform validation: $PLATFORM_TEST_STATUS

This package installs the API and Judge Worker as systemd services and serves
the Vue frontend through Nginx when available. Judge Runtime images are
published separately as project-balloon-$VERSION-$PACKAGE_TARGET-judge-images.tar.gz;
download and extract that archive, then pass its extracted judge-images
directory to install.sh with --judge-images. PostgreSQL, Redis, RabbitMQ,
RustFS, Docker/Podman, CUPS, and Nginx must be supplied by the operator.

Install as root:
  ./install.sh
  ./install.sh --judge-images judge-images
For separated hosts:
  ./install.sh --role api
  ./install.sh --role worker --skip-nginx --container-group docker \
    --judge-images judge-images
Edit /etc/project-balloon/project-balloon.env, then run ./install.sh again.

Only Linux x86_64 has been tested end to end. Other platform packages are
build and package checks only until their runtime and installation workflows
have been validated on the target host.
EOF

(cd "$TMP" && find . -type f ! -name PACKAGE-SHA256SUMS -print0 | sort -z \
  | xargs -0 sha256sum) > "$TMP.PACKAGE-SHA256SUMS"
mv "$TMP.PACKAGE-SHA256SUMS" "$TMP/PACKAGE-SHA256SUMS"
mkdir -p "$OUT_BASE"
mv "$TMP" "$FINAL"
printf '[release] binary package ready: %s\n' "$FINAL"
