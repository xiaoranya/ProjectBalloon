FROM debian:bookworm-slim

RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get update; \
    apt-get install -y --no-install-recommends gcc-12 g++-12 libc6-dev time; \
    rm -rf /var/lib/apt/lists/*; \
    update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-12 100; \
    update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-12 100; \
    g++-12 --version | head -1 | awk '{print $NF}' > /VERSION; \
    /usr/bin/time --version | head -1

RUN groupadd --gid 1000 judge \
    && useradd --uid 1000 --gid 1000 --create-home --shell /bin/bash judge

WORKDIR /work
USER judge
