#!/usr/bin/env bash
set -euo pipefail

readonly container_name="rtmp-manager-test-${$}"

cleanup() {
    docker stop "${container_name}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

command -v docker >/dev/null || {
    echo "docker is required" >&2
    exit 1
}
command -v ffmpeg >/dev/null || {
    echo "ffmpeg is required" >&2
    exit 1
}

docker run \
    --rm \
    --detach \
    --name "${container_name}" \
    --publish 127.0.0.1:1935:1935 \
    bluenviron/mediamtx:1.20.1 >/dev/null

for _ in $(seq 1 50); do
    if (echo >/dev/tcp/127.0.0.1/1935) 2>/dev/null; then
        cargo test --locked \
            server::relay::tests::direct_test_publishes_to_local_rtmp_server \
            -- \
            --ignored \
            --exact
        exit
    fi
    sleep 0.1
done

echo "MediaMTX did not open port 1935" >&2
docker logs "${container_name}" >&2
exit 1
