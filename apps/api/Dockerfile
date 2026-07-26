FROM rust:1.94.1-bookworm AS build

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --locked --release -p project-balloon-api

FROM debian:bookworm-slim

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates cups-client cups-filters wget; \
    rm -rf /var/lib/apt/lists/*; \
    groupadd --gid 10001 app; \
    useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin app

COPY --from=build /workspace/target/release/project-balloon-api /usr/local/bin/project-balloon-api

USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/project-balloon-api"]
