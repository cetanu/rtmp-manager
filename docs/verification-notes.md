# Verification notes and open acceptance questions

This file records checks that cannot be proven by the local test suite.

| Goal | Question / limitation | Required evidence |
| --- | --- | --- |
| 101–104 | Docker and FFmpeg-in-container smoke tests cannot run in this environment because the Docker CLI is unavailable. | Run `docker compose up` and publish RTMP/SRT media on both amd64 and arm64 runners. |
| 201–204 | KMS-backed key management has no configured provider in this environment. | Exercise startup, rotation, and decrypt-on-read against the selected KMS deployment. |
| 301–303 | Multi-node worker fencing and reconnect/slate behavior require a broker and live media sources. | Soak multiple workers through broker loss, ingest disconnect, and reconnect. |
| 401–403 | Provider quota, webhook fleet, and bot relay behavior require real platform credentials. | Run provider sandbox/integration tests with tenant-isolated accounts. |
| 501–503 | Billing lifecycle, collector export, and production authorization review require deployed external services. | Validate subscription events, scrape/export configuration, and incident permissions in staging. |

These are deployment or provider acceptance checks, not silently skipped unit
tests; the implementation and local invariants remain covered by CI tests.
