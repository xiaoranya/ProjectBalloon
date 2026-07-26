#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
OUT="${1:-$ROOT/images}"
mkdir -p "$OUT"

images=(
  "project-balloon-api:$VERSION"
  "project-balloon-judge-worker:$VERSION"
  "project-balloon-web:$VERSION"
  postgres:16.6 redis:7.4.2 rabbitmq:3.13.7-management rustfs/rustfs:1.0.0-beta.8
  prom/prometheus:v3.1.0 grafana/grafana:11.4.0 grafana/loki:3.3.0 grafana/promtail:3.3.0
  prometheuscommunity/postgres-exporter:v0.16.0 prom/node-exporter:v1.8.2
  judge-runtime-c:12.2.0 judge-runtime-cpp:12.2.0 judge-runtime-java:21 judge-runtime-python:3.12.13
)

rm -f "$OUT"/*.tar "$OUT/SHA256SUMS"
for image in "${images[@]}"; do
  docker image inspect "$image" >/dev/null
  filename="$(printf '%s' "$image" | tr '/:' '__').tar"
  printf '[export] %s -> %s\n' "$image" "$filename"
  docker save --output "$OUT/$filename" "$image"
done
(cd "$OUT" && sha256sum ./*.tar > SHA256SUMS)
