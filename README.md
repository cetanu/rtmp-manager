<img src="./rtmp.png" alt="rtmp-manager logo" width="180">

# rtmp-manager

A self-hosted live-stream multiplexer and management dashboard. It accepts one
RTMP ingest and relays it to configured destinations such as Twitch, YouTube,
and X.

The Rust service includes a server-rendered Topcoat dashboard for stream
targets, notifications, access credentials, and chat integrations. Application
configuration and chat state are stored in SQLite.

## Chat layout regression test

The browser test measures the chat panel against the available viewport on
desktop and mobile. It checks direct loading, client navigation, live messages,
internal scrolling, resizing, and browser history using an isolated database
and ports.

With Rust 1.98 or newer and the Topcoat 0.10 CLI installed:

```sh
python3 -m venv /tmp/rtmp-browser-tests
/tmp/rtmp-browser-tests/bin/pip install -r tests/browser/requirements.txt
/tmp/rtmp-browser-tests/bin/python -m playwright install --with-deps chromium
PATH="/tmp/rtmp-browser-tests/bin:$PATH" just test-chat-layout
```
