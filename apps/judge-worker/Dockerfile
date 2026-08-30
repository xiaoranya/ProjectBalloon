FROM rust:1.94.1-bookworm AS build

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
RUN cargo build --locked --release -p project-balloon-judge-worker

FROM debian:bookworm-slim

RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates procps wget; \
    rm -rf /var/lib/apt/lists/*; \
    groupadd --gid 1000 judge; \
    useradd --uid 1000 --gid 1000 --create-home --shell /usr/sbin/nologin judge

COPY --from=build /workspace/target/release/project-balloon-judge-worker /usr/local/bin/project-balloon-judge-worker

USER 1000:1000
ENTRYPOINT ["/usr/local/bin/project-balloon-judge-worker"]
