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

Copy `config.example.json` to `config.json` before running locally.

## Deployment

The `salt` directory is a reusable Salt formula. The infrastructure host mounts
this repository at `salt://rtmp-manager` through GitFS and applies the
`rtmp-manager` state after a signed GitHub release webhook.

Production secrets are stored as GPG-encrypted Git pillar data under `pillar`.
See [`pillar/README.md`](pillar/README.md) for setup. Only the deployment public
key belongs in this repository; the private key remains on the host.

## Release deployment webhook

The release workflow triggers deployment only after the `rtmp-proxy` binary and
its checksum have been attached to the GitHub release. Configure these Actions
repository secrets:

- `DEPLOYMENT_WEBHOOK_URL`: the complete HTTPS endpoint, including the webhook
  path (for example, `https://deploy.example.com/hooks/github`)
- `DEPLOYMENT_WEBHOOK_SECRET`: exactly the value configured as
  `webhookSecret` in the infrastructure Pulumi stack

## Kick chat webhook

Webhooks are received at `POST /api/webhook` and broadcast internally to
platform integrations. The Kick integration subscribes to that broadcast and
only accepts signed `chat.message.sent` events. When configuring a Kick
developer app, set this as its HTTPS webhook URL and subscribe the authorized
channel to `chat.message.sent`. The app registration and OAuth credentials are
not stored in this repository.
