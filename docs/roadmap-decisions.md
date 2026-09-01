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

## GOAL-302: relay limits

- Resource limits are opt-in environment settings so existing self-hosted
  deployments remain portable. Linux workers use `prlimit`; other platforms
  continue with the supervisor's cancellation and restart guarantees.
