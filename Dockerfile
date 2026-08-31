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
        ffmpeg \
        libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 --user-group rtmp-manager \
    && install -d -o rtmp-manager -g rtmp-manager /data /opt/rtmp-manager

COPY --from=builder --chown=rtmp-manager:rtmp-manager /output/rtmp-proxy /opt/rtmp-manager/rtmp-proxy
COPY --chown=rtmp-manager:rtmp-manager config.example.json /data/config.json

USER rtmp-manager
WORKDIR /data
ENV CONFIG_PATH=/data/config.json \
    RUST_LOG=rtmp_proxy=info,rtmp_rs=off

VOLUME ["/data"]
EXPOSE 1935 3000 6000/udp

ENTRYPOINT ["/opt/rtmp-manager/rtmp-proxy"]
