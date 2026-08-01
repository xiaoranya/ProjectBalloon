#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
STAGING="$ROOT/dist/binary-staging"
JUDGE_IMAGES="${1:-$ROOT/judge-images}"
OUT_BASE="${2:-$ROOT/dist}"

[ -d "$STAGING/bin" ] || { echo 'build binary staging before packaging' >&2; exit 1; }
TARGET="${PROJECT_BALLOON_BINARY_TARGET:-$(sed -n 's/^PROJECT_BALLOON_TARGET=//p' "$STAGING/metadata/release.env")}"
[ -n "$TARGET" ] || { echo 'binary target is missing from staging metadata' >&2; exit 1; }
[ -f "$JUDGE_IMAGES/SHA256SUMS" ] || { echo 'export judge images before packaging' >&2; exit 1; }
(cd "$JUDGE_IMAGES" && sha256sum -c SHA256SUMS)
FINAL="$OUT_BASE/project-balloon-$VERSION-$TARGET"
TMP="$FINAL.tmp"
[ ! -e "$FINAL" ] && [ ! -e "$TMP" ] || { echo "output already exists: $FINAL" >&2; exit 1; }

cleanup() {
  local status="$?"
  if [ "$status" -ne 0 ]; then
    rm -rf "$TMP" "$TMP.PACKAGE-SHA256SUMS"
  fi
}
trap cleanup EXIT

mkdir -p "$TMP/bin" "$TMP/web" "$TMP/judge-images" "$TMP/config" \
  "$TMP/systemd" "$TMP/nginx" "$TMP/lib" "$TMP/docs" "$TMP/scripts/backup" "$TMP/scripts/lib"
cp -a "$STAGING/bin/." "$TMP/bin/"
cp -a "$STAGING/web/." "$TMP/web/"
cp -a "$STAGING/metadata/." "$TMP/config/"
cp -a "$JUDGE_IMAGES/." "$TMP/judge-images/"
cp "$ROOT/install.sh" "$TMP/install.sh"
cp "$ROOT/deploy/binary/project-balloon.env.example" "$TMP/config/"
cp "$ROOT/deploy/binary/bootstrap-admin.env.example" "$TMP/config/"
cp "$ROOT/deploy/binary/project-balloon-api.service" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon-judge-worker.service" "$TMP/systemd/"
cp "$ROOT/deploy/binary/project-balloon.nginx.conf" "$TMP/nginx/"
cp "$ROOT/scripts/lib/env-file.sh" "$TMP/lib/"
cp "$ROOT/scripts/lib/deploy-common.sh" "$TMP/scripts/lib/"
cp "$ROOT/scripts/lib/env-file.sh" "$TMP/scripts/lib/"
cp -a "$ROOT/docs/ops/." "$TMP/docs/"
cp -a "$ROOT/scripts/backup/." "$TMP/scripts/backup/"
cp "$ROOT/VERSION" "$TMP/"

cat > "$TMP/README.txt" <<EOF
ProjectBalloon binary release $VERSION ($TARGET)

This package installs the API and Judge Worker as systemd services, serves the
Vue frontend through Nginx when available, and imports the four Judge Runtime
images. PostgreSQL, Redis, RabbitMQ, RustFS, Docker/Podman, CUPS, and Nginx
must be supplied by the operator.

Install as root:
  ./install.sh
Edit /etc/project-balloon/project-balloon.env, then run ./install.sh again.
EOF

(cd "$TMP" && find . -type f ! -name PACKAGE-SHA256SUMS -print0 | sort -z \
  | xargs -0 sha256sum) > "$TMP.PACKAGE-SHA256SUMS"
mv "$TMP.PACKAGE-SHA256SUMS" "$TMP/PACKAGE-SHA256SUMS"
mkdir -p "$OUT_BASE"
mv "$TMP" "$FINAL"
printf '[release] binary package ready: %s\n' "$FINAL"
