#!/usr/bin/env bash
set -euo pipefail

readonly container_name="rtmp-manager-dashboard-${$}"
readonly state_dir="$(mktemp -d "${TMPDIR:-/tmp}/rtmp-manager-dashboard.XXXXXX")"
readonly config_path="${state_dir}/config.sqlite3"
app_pid=""

cleanup() {
    if [[ -n "${app_pid}" ]]; then
        kill "${app_pid}" >/dev/null 2>&1 || true
        sleep 0.2
        kill -KILL "${app_pid}" >/dev/null 2>&1 || true
        wait "${app_pid}" >/dev/null 2>&1 || true
    fi
    docker stop --time 1 "${container_name}" >/dev/null 2>&1 || true
    rm -rf "${state_dir}"
}

shutdown() {
    trap - EXIT INT TERM
    cleanup
    exit 130
}

trap cleanup EXIT
trap shutdown INT TERM

command -v docker >/dev/null || {
    echo "docker is required" >&2
    exit 1
}
command -v ffmpeg >/dev/null || {
    echo "ffmpeg is required" >&2
    exit 1
}
command -v curl >/dev/null || {
    echo "curl is required" >&2
    exit 1
}

docker run \
    --rm \
    --detach \
    --name "${container_name}" \
    --publish 127.0.0.1:1936:1935 \
    bluenviron/mediamtx:1.20.1 >/dev/null

cargo run --locked -- --config "${config_path}" &
app_pid=$!

for _ in $(seq 1 300); do
    if ! kill -0 "${app_pid}" 2>/dev/null; then
        wait "${app_pid}"
        exit 1
    fi
    if curl --fail --silent http://127.0.0.1:3000/api/config >/dev/null 2>&1; then
        if ! curl \
            --fail-with-body \
            --silent \
            --show-error \
            --request POST \
            --header 'Content-Type: application/json' \
            --data-binary @scripts/test-stream-dashboard.json \
            http://127.0.0.1:3000/api/config/import; then
            echo >&2
            echo "Failed to load the local dashboard configuration" >&2
            exit 1
        fi
        echo
        echo "Dashboard: http://127.0.0.1:3000/targets"
        echo "Username:  local"
        echo "Password:  local-testing"
        echo
        echo "Press Test Stream, then open Metrics to watch the local target."
        echo "Press Ctrl-C here to stop the dashboard and RTMP server."
        wait "${app_pid}"
        exit
    fi
    sleep 0.1
done

echo "Dashboard did not open port 3000" >&2
exit 1
