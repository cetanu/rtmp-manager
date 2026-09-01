#!/bin/sh
set -eu

key_file=/data/.master-encryption-key
if [ -z "${MASTER_ENCRYPTION_KEY:-}" ]; then
    if [ -s "$key_file" ]; then
        MASTER_ENCRYPTION_KEY=$(cat "$key_file")
    else
        umask 077
        MASTER_ENCRYPTION_KEY=$(head -c 48 /dev/urandom | base64 | tr -d '\n=/+')
        printf '%s\n' "$MASTER_ENCRYPTION_KEY" > "$key_file"
    fi
    export MASTER_ENCRYPTION_KEY
fi

exec /opt/rtmp-manager/rtmp-proxy "$@"
