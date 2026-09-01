# Roadmap decisions

## GOAL-202: accounts and OAuth

- Local registration creates a new tenant and makes the registering account
  its administrator. This keeps stream keys, chat, overlays, and publishing
  targets isolated without requiring a separate provisioning workflow.
- OAuth providers are enabled only when their documented environment
  variables are present. The application does not persist provider secrets;
  it stores only the provider subject and a verified email association.
- Passwords use Argon2id and sessions are opaque, revocable database-backed
  tokens. The previous shared Basic Auth path is removed.

## GOAL-402: webhook tenancy

- Platform webhooks identify their tenant with the `X-Tenant-Stream-Key`
  header. This is the smallest deployable two-way contract while provider
  account-to-tenant registration is still local; signatures are then verified
  against that tenant's stored settings.
- The unified endpoint uses explicit platform paths (`/api/v1/webhooks/kick`
  and `/api/v1/webhooks/x`) so malformed or unknown platform payloads cannot
  select a parser implicitly.
- Twitch EventSub uses `TWITCH_EVENTSUB_SECRET` for HMAC verification and the
  same `X-Tenant-Stream-Key` routing header; this keeps provider signatures
  independent from tenant lookup while the control plane has no provider
  account registry yet.

## GOAL-302: relay limits

- Resource limits are opt-in environment settings so existing self-hosted
  deployments remain portable. Linux workers use `prlimit`; other platforms
  continue with the supervisor's cancellation and restart guarantees.
- Relay FFmpeg processes are considered stalled after 15 seconds without any
  `-progress` output. The supervisor terminates the process and reuses its
  bounded reconnect backoff; this timeout is intentionally fixed until
  provider-specific stream health signals are available.

## GOAL-304: hardware encoding

- Hardware encoders are explicitly allow-listed (`nvenc`, `vaapi`, `qsv`, and
  `videotoolbox`) and map to FFmpeg's native H.264 encoders. The host image is
  responsible for exposing the matching device/runtime.
- Target settings expose the encoding mode, bitrate cap, dimensions, and
  hardware encoder fields; submitted values are parsed and validated at the
  configuration boundary before relays start.
- CPU and hardware encoded outputs force AAC at 128 kbps, 48 kHz, stereo so
  surround or incompatible source audio layouts do not break destination
  publishing.
- Requested dimensions use aspect-preserving enlargement followed by a
  centered crop, making vertical profiles usable without stretching the
  source image.

## GOAL-303: disconnect grace

- The disconnect grace period is a validated server setting (0–300 seconds),
  defaulting to 30 seconds and applied by the stream handle when scheduling
  delayed teardown. A zero value intentionally ends a stream immediately.
- During that window, active targets are swapped to a generated black-video,
  silent-audio lavfi relay. A successful republish cancels the standby jobs
  and starts live relays again; explicit stop and expiry still tear everything
  down.
- Standby output also honors each target's configured dimensions and video
  bitrate cap, keeping the reconnect feed within the destination profile.
- Standby jobs retry failed FFmpeg connections once per second until the
  ingest stream republishes or its grace window expires.

## GOAL-204: database health

- `/healthz` is unauthenticated so orchestrators can probe the service before
  login; it executes a live `SELECT 1` against the configured SQLx pool and
  returns `503` when the database is unavailable.
- The runtime image uses curl for a Docker `HEALTHCHECK` against `/healthz`,
  allowing Compose and schedulers to restart unhealthy instances.
- The container entrypoint accepts an explicit `MASTER_ENCRYPTION_KEY`; when
  omitted, it generates a random key once and persists it in `/data` with
  restrictive permissions so first-run setup remains one-command while
  encrypted data survives restarts.
- `MASTER_ENCRYPTION_KEY_FILE` is also supported for mounted secret files;
  direct environment configuration takes precedence when both are present.

## GOAL-501: usage quotas

- Quotas are enforced independently of payment-provider integrations. The
  initial plans are free (10 hours/month), pro (100 hours/month), and
  enterprise (unlimited); a later billing adapter can update the plan column
  without changing stream lifecycle code.
- Active streams are recorded separately from accumulated seconds so live
  reservations remain inspectable and are removed on every stream teardown.
- Provider webhooks use `BILLING_WEBHOOK_SECRET` and a generic signed payload;
  Stripe/LemonSqueezy adapters can translate their events into this contract.
- Stripe has a dedicated `/api/billing/stripe` adapter using its five-minute
  signed-event window and tenant/plan metadata on subscription objects.
- LemonSqueezy has a dedicated `/api/billing/lemonsqueezy` adapter using its
  raw-body `X-Signature` HMAC and `meta.custom_data` tenant metadata.

## GOAL-401: YouTube quota partitioning

- Each tenant's encrypted YouTube API key is passed to the polling worker and
  takes precedence over the public browser key discovered during chat-page
  bootstrap. The browser key remains the explicit fallback for installations
  that do not provide a tenant credential.
- Poll requests use independent bounded semaphores per API key, preventing one
  tenant's credential budget from serializing unrelated tenants.

## GOAL-403: chat relaying

- The first outbound adapter is Twitch IRC, configured with
  `TWITCH_BOT_OAUTH_TOKEN`, `TWITCH_BOT_USERNAME`, and
  `CHAT_RELAY_TWITCH_CHANNEL`. Relay messages are prefixed and newline
  sanitized; messages originating on the destination platform are skipped to
  prevent loops.
- Kick and YouTube adapters use their official chat APIs with
  `KICK_BOT_OAUTH_TOKEN` and `YOUTUBE_BOT_OAUTH_TOKEN`; selecting a destination
  is done with `CHAT_RELAY_DESTINATION` and source filtering with
  `CHAT_RELAY_SOURCE`.
- The dispatcher permits at most five relayed messages per destination in a
  ten-second window, providing a bounded spam guard before provider APIs are
  called.
- Relay source/destination settings are stored with each tenant's encrypted
  chat configuration; environment variables remain an explicit deployment
  override for installations managed outside the dashboard.
- The dashboard exposes those source, destination, and enabled fields so a
  tenant can configure relay rules without editing environment files.

## GOAL-502: observability

- Relay lifecycle is emitted as a structured `relay.worker` tracing span with
  tenant and target attributes, allowing any OpenTelemetry-compatible tracing
  subscriber/collector to export it without hard-coding a vendor SDK.
- `/api/metrics/prometheus` is the scrape boundary; it emits only the
  authenticated tenant's ingest and outbound bitrate series.
- Relay progress now records FFmpeg dropped-frame totals and reconnection
  counts alongside outbound bitrate, and exposes both in the tenant-scoped
  Prometheus response.
- The metrics dashboard renders those QoS counters per target and refreshes
  them from the same tenant-filtered history endpoint as bitrate charts.

## GOAL-503: moderation

- Administrators have both a global emergency stop and a tenant-scoped
  `/api/admin/tenants/{tenant_id}/emergency-stop` endpoint, allowing targeted
  intervention without disrupting unrelated broadcasts.

## GOAL-301: relay control plane

- Redis Streams is the first broker adapter because it is easy to self-host;
  `RELAY_BROKER_URL` selects it while the local executor remains the default
  for installations without a broker.
- Workers consume through a Redis consumer group and acknowledge intents only
  after starting or stopping the associated relay; this provides redelivery
  after a worker crash without coupling the control plane to one process.
- Workers reclaim entries idle for 30 seconds with `XAUTOCLAIM`, allowing a
  replacement worker to recover jobs left pending by a crashed consumer.
- The Compose broker profile health-checks Redis before starting workers and
  restarts workers automatically, removing the cold-start dependency race.
- During control-plane shutdown, broker-backed relay handles are detached
  instead of sending stop intents; explicit stops and emergency kills still
  send stop intents, so API restarts do not interrupt active broadcasts.
