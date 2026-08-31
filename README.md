# rtmp-manager

A self-hosted live-stream multiplexer and management dashboard. It accepts one
RTMP ingest and relays it to configured destinations such as Twitch, YouTube,
and X.

The Rust service includes a server-rendered Topcoat dashboard for stream
targets, notifications, access credentials, and chat integrations. Application
configuration and chat state are stored in SQLite.

## Run with Docker Compose

Start the dashboard and RTMP ingest listener with:

```sh
docker compose up --build
```

The dashboard is available at <http://localhost:3000>. Publish from OBS to
`rtmp://localhost:1935/live/CHANGE_ME_TO_A_PRIVATE_STREAM_KEY`, then replace the
placeholder ingest and destination keys in the dashboard before using the
service publicly.

The `rtmp-manager-data` volume persists the seed configuration and SQLite
database. Set `HTTP_PORT`, `RTMP_PORT`, or `RUST_LOG` in the shell or a `.env`
file to override the corresponding Compose defaults. To use a prebuilt image,
remove the `build` block or run `docker compose pull` after images have been
published to GHCR.

## OBS chat overlay

After first-run setup, open Settings and copy the private OBS Browser Source URL
from **OBS Chat Overlay**. The transparent overlay receives aggregated chat over
Server-Sent Events and supports Dark, Minimal, Comic, and Transparent Box
themes plus font size, message opacity, badges, avatars, emoji, and fade timing.
Keep the overlay URL private because its query key grants read access to chat.
