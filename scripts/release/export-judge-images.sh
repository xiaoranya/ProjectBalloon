#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/judge-images}"
mkdir -p "$OUT"

images=(
  judge-runtime-c:12.2.0
  judge-runtime-cpp:12.2.0
  judge-runtime-java:21
  judge-runtime-python:3.12.13
  judge-runtime-go:1.24
  judge-runtime-rust:1.88
)

rm -f "$OUT"/*.tar "$OUT/SHA256SUMS"
for image in "${images[@]}"; do
  docker image inspect "$image" >/dev/null
  filename="$(printf '%s' "$image" | tr '/:' '__').tar"
  printf '[export] %s -> %s\n' "$image" "$filename"
  docker save --output "$OUT/$filename" "$image"
done
(cd "$OUT" && sha256sum ./*.tar > SHA256SUMS)
