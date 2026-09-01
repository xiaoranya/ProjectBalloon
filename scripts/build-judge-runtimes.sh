#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_ROOT="$PROJECT_ROOT/deploy/judge/runtimes"

docker build --pull=false -t judge-runtime-c:12.2.0 "$RUNTIME_ROOT/c"
docker build --pull=false -t judge-runtime-cpp:12.2.0 "$RUNTIME_ROOT/cpp"
docker build --pull=false -t judge-runtime-java:21 "$RUNTIME_ROOT/java"
docker build --pull=false -t judge-runtime-python:3.12.13 "$RUNTIME_ROOT/python"
docker build --pull=false -t judge-runtime-go:1.24 "$RUNTIME_ROOT/go"
docker build --pull=false -t judge-runtime-rust:1.88 "$RUNTIME_ROOT/rust"

for image in \
    judge-runtime-c:12.2.0 \
    judge-runtime-cpp:12.2.0 \
    judge-runtime-java:21 \
    judge-runtime-python:3.12.13 \
    judge-runtime-go:1.24 \
    judge-runtime-rust:1.88
do
    docker run --rm --entrypoint /usr/bin/time "$image" --version | head -1
done
