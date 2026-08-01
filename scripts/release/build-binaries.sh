#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
TARGET="${PROJECT_BALLOON_BINARY_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
PACKAGE_TARGET="${PROJECT_BALLOON_PACKAGE_TARGET:-$TARGET}"
STAGING="$ROOT/dist/binary-staging"

[ -n "$TARGET" ] || { echo 'could not determine Rust target' >&2; exit 1; }
[ -n "$PACKAGE_TARGET" ] || { echo 'binary package target is empty' >&2; exit 1; }
rm -rf "$STAGING"
mkdir -p "$STAGING/bin" "$STAGING/web" "$STAGING/metadata"

cargo build --locked --release --workspace --all-features --target "$TARGET"

BINARY_SUFFIX=""
case "$TARGET" in
  *-pc-windows-*) BINARY_SUFFIX='.exe' ;;
esac
BINARY_DIR="$ROOT/target/$TARGET/release"

for binary in project-balloon-api project-balloon-judge-worker bootstrap-admin; do
  source="$BINARY_DIR/$binary$BINARY_SUFFIX"
  [ -f "$source" ] || {
    echo "missing release binary: $source" >&2
    exit 1
  }
  cp "$source" "$STAGING/bin/$binary$BINARY_SUFFIX"
  chmod 0755 "$STAGING/bin/$binary$BINARY_SUFFIX"
done

npm --prefix "$ROOT/frontend/web" run build
cp -a "$ROOT/frontend/web/dist/." "$STAGING/web/"

cat > "$STAGING/metadata/release.env" <<EOF
PROJECT_BALLOON_VERSION=$VERSION
PROJECT_BALLOON_TARGET=$TARGET
PROJECT_BALLOON_PACKAGE_TARGET=$PACKAGE_TARGET
PROJECT_BALLOON_BINARY_SUFFIX=$BINARY_SUFFIX
PROJECT_BALLOON_PLATFORM_TEST_STATUS=${PROJECT_BALLOON_PLATFORM_TEST_STATUS:-linux-x86_64-only}
EOF

printf '[release] binary staging ready: version=%s target=%s package_target=%s\n' \
  "$VERSION" "$TARGET" "$PACKAGE_TARGET"
