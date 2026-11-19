<img src="./crates/web/rtmp.png" alt="rtmp-manager logo" width="180">

# rtmp-manager

A self-hosted live-stream multiplexer and management dashboard. It accepts one
RTMP ingest and relays it to configured destinations.

## Host deployment

`deploy/apply.py` installs the prebuilt release using shared helpers from
`host-services`. Configuration lives in `deploy/config.yaml`. Invoke deployment
through `sudo systemctl start deployment-reconcile.service`; `stream-infra` owns
the serialized runner and its memory limits.
