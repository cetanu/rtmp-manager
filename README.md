# rtmp-manager

A self-hosted live-stream multiplexer and management dashboard. It accepts one
RTMP ingest and relays it to configured destinations such as Twitch, YouTube,
and X.

The Rust service includes a server-rendered Topcoat dashboard for stream
targets, notifications, access credentials, and chat integrations. Application
configuration and chat state are stored in SQLite or PostgreSQL.

## Run with Docker Compose

Start the dashboard and RTMP ingest listener with:

```sh
docker compose up --build
```

The dashboard is available at <http://localhost:3000>. Publish RTMP from OBS to
`rtmp://localhost:1935/live/CHANGE_ME_TO_A_PRIVATE_STREAM_KEY`, then replace the
placeholder ingest and destination keys in the dashboard before using the
service publicly.

SRT ingest listens on UDP port 6000. Configure OBS with
`srt://localhost:6000?mode=caller&streamid=#!::r=live,m=publish,u=<stream_key>`.
The access-control stream ID is authenticated before MPEG-TS packets are
remuxed into the same relay pipeline as RTMP ingest.

The `rtmp-manager-data` volume persists the SQLite database and generated
encryption key. Set `MASTER_ENCRYPTION_KEY` to a strong, stable secret when
you need externally managed key material; otherwise the container generates a
random key once in the volume. The key encrypts destination stream keys and
OAuth state at rest. Secret managers can mount the key and set
`MASTER_ENCRYPTION_KEY_FILE` instead. On Linux, `RELAY_MAX_CPU_SECONDS` and
`RELAY_MAX_MEMORY_MB` optionally apply per-relay `prlimit` caps to FFmpeg
workers. Set `HTTP_PORT`,
`RTMP_PORT`, `SRT_PORT`, or `RUST_LOG` in the shell or a `.env` file to override
the corresponding Compose defaults. Set `DATABASE_URL` to a PostgreSQL URL to
use PostgreSQL instead. Both backends run embedded schema migrations during
startup. To use a prebuilt image, remove the `build` block or run
`docker compose pull` after images have been published to GHCR.

For broker-backed media workers, start the optional Redis and worker profile
with `docker compose --profile broker up` and set
`RELAY_BROKER_URL=redis://relay-broker/` on the manager service.

## Relay resilience and encoding

Server Settings controls the ingest disconnect grace period (0–300 seconds;
30 seconds by default). During that window, enabled targets receive a
profile-aware black/silent standby feed and automatically return to live media
when ingest republishes. Target settings also support passthrough or encoded
profiles with bitrate caps, aspect-preserving dimensions, and allow-listed
hardware encoders (`nvenc`, `vaapi`, `qsv`, or `videotoolbox`).

## OBS chat overlay

After first-run setup, open Settings and copy the private OBS Browser Source URL
from **OBS Chat Overlay**. The transparent overlay receives aggregated chat over
Server-Sent Events and supports Dark, Minimal, Comic, and Transparent Box
themes plus font size, message opacity, badges, avatars, emoji, and fade timing.
Keep the overlay URL private because its query key grants read access to chat.

Authenticated operators can scrape tenant-scoped Prometheus metrics from
`/api/metrics/prometheus`.

## Stripe billing (optional)

Set `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRO_PRICE_ID`,
`STRIPE_ENTERPRISE_PRICE_ID`, `STRIPE_CHECKOUT_SUCCESS_URL`,
`STRIPE_CHECKOUT_CANCEL_URL`, and `STRIPE_PORTAL_RETURN_URL` to enable hosted
Checkout and Customer Portal sessions. Configure Stripe to deliver subscription
events to `/api/billing/stripe`; the webhook metadata must include `tenant_id`
and `plan` so feature access updates immediately.

Contributors can run the complete local formatting, test, and lint gates with
`make verify`.
