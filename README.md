# rtmp-manager

A self-hosted live-stream multiplexer and management dashboard. It accepts one
RTMP ingest and relays it to configured destinations such as Twitch, YouTube,
and X.

The Rust service includes a server-rendered Topcoat dashboard for stream
targets, notifications, access credentials, and chat integrations. Application
configuration and chat state are stored in SQLite.

## Development

```sh
cargo run
```

Copy `config.example.toml` to `config.toml` before running locally.

## Deployment

The `salt` directory is a reusable Salt formula. The infrastructure host mounts
this repository at `salt://rtmp-manager` through GitFS and applies the
`rtmp-manager` state after a signed GitHub release webhook.

Production secrets are stored as GPG-encrypted Git pillar data under `pillar`.
See [`pillar/README.md`](pillar/README.md) for setup. Only the deployment public
key belongs in this repository; the private key remains on the host.
