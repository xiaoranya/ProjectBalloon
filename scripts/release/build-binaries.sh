#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
TARGET="${PROJECT_BALLOON_BINARY_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
STAGING="$ROOT/dist/binary-staging"

[ -n "$TARGET" ] || { echo 'could not determine Rust target' >&2; exit 1; }
rm -rf "$STAGING"
mkdir -p "$STAGING/bin" "$STAGING/web" "$STAGING/metadata"

cargo build --locked --release --workspace --all-features

for binary in project-balloon-api project-balloon-judge-worker bootstrap-admin; do
  [ -x "$ROOT/target/release/$binary" ] || {
    echo "missing release binary: target/release/$binary" >&2
    exit 1
  }
  install -m 0755 "$ROOT/target/release/$binary" "$STAGING/bin/$binary"
done

npm --prefix "$ROOT/frontend/web" run build
cp -a "$ROOT/frontend/web/dist/." "$STAGING/web/"

cat > "$STAGING/metadata/release.env" <<EOF
PROJECT_BALLOON_VERSION=$VERSION
PROJECT_BALLOON_TARGET=$TARGET
EOF

printf '[release] binary staging ready: version=%s target=%s\n' "$VERSION" "$TARGET"
