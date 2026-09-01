FROM golang:1.24-bookworm

RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get update; \
    apt-get install -y --no-install-recommends time; \
    rm -rf /var/lib/apt/lists/*; \
    go version | head -1 | awk '{print $NF}' > /VERSION; \
    /usr/bin/time --version | head -1

RUN groupadd --gid 1000 judge \
    && useradd --uid 1000 --gid 1000 --create-home --shell /bin/bash judge

WORKDIR /work
USER judge
