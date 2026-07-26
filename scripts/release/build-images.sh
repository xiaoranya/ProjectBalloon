#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${PROJECT_BALLOON_IMAGE_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"

docker build --pull=false -t "project-balloon-api:$VERSION" -f "$ROOT/apps/api/Dockerfile" "$ROOT"
docker build --pull=false -t "project-balloon-judge-worker:$VERSION" -f "$ROOT/apps/judge-worker/Dockerfile" "$ROOT"
docker build --pull=false -t "project-balloon-web:$VERSION" -f "$ROOT/frontend/web/Dockerfile" "$ROOT"
"$ROOT/scripts/build-judge-runtimes.sh"
