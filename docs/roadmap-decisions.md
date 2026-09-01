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

## GOAL-304: hardware encoding

- Hardware encoders are explicitly allow-listed (`nvenc`, `vaapi`, `qsv`, and
  `videotoolbox`) and map to FFmpeg's native H.264 encoders. The host image is
  responsible for exposing the matching device/runtime.

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

## GOAL-502: observability

- Relay lifecycle is emitted as a structured `relay.worker` tracing span with
  tenant and target attributes, allowing any OpenTelemetry-compatible tracing
  subscriber/collector to export it without hard-coding a vendor SDK.
- `/api/metrics/prometheus` is the scrape boundary; it emits only the
  authenticated tenant's ingest and outbound bitrate series.

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
