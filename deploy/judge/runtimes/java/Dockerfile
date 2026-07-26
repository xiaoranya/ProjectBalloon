FROM eclipse-temurin:21-jdk-jammy

RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive; \
    apt-get update; \
    apt-get install -y --no-install-recommends time; \
    rm -rf /var/lib/apt/lists/*; \
    ln -sf /opt/java/openjdk/bin/javac /usr/local/bin/javac; \
    ln -sf /opt/java/openjdk/bin/java /usr/local/bin/java; \
    /usr/bin/time --version | head -1

RUN groupadd --gid 1000 judge \
    && useradd --uid 1000 --gid judge --shell /bin/bash --create-home judge

WORKDIR /work
USER judge
