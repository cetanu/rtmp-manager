# syntax=docker/dockerfile:1.7

FROM docker.io/library/rust:bookworm AS builder

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential \
        cmake \
        libsqlite3-dev \
        ninja-build \
        perl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release --bin rtmp-proxy \
    && install -D target/release/rtmp-proxy /output/rtmp-proxy

FROM docker.io/library/debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        curl \
        ffmpeg \
        libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 --user-group rtmp-manager \
    && install -d -o rtmp-manager -g rtmp-manager /data /opt/rtmp-manager

COPY --from=builder --chown=rtmp-manager:rtmp-manager /output/rtmp-proxy /opt/rtmp-manager/rtmp-proxy

USER rtmp-manager
WORKDIR /data
ENV DATABASE_URL=sqlite:///data/rtmp-manager.sqlite3?mode=rwc \
    RUST_LOG=rtmp_proxy=info,rtmp_rs=off

VOLUME ["/data"]
EXPOSE 1935 3000 6000/udp

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["curl", "--fail", "--silent", "http://127.0.0.1:3000/healthz"]

ENTRYPOINT ["/opt/rtmp-manager/rtmp-proxy"]
