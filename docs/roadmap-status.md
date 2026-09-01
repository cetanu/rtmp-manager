# Roadmap evidence matrix

This matrix maps each roadmap goal to the current implementation and the
remaining acceptance work. It is intentionally kept next to the code so the
draft PR does not imply completion where an external service is still needed.

| Goal | Current evidence | Remaining acceptance work |
| --- | --- | --- |
| 101–104 | Docker/Compose, persisted key bootstrap, runtime healthcheck, setup, overlay, SRT listener | Live provider/container streaming smoke tests in CI |
| 201–204 | Tenant repository, accounts/RBAC, AES-GCM, env/file key sources, SQLx migrations, and `/healthz` pool checks | Production key-management/KMS integration |
| 301–302 | RelayExecutor, Redis Streams worker, readiness-gated Compose broker, restart-safe relay detachment, ACK/claim, prlimit | Multi-node deployment soak and worker fencing |
| 303–304 | Configurable disconnect grace with profile-aware, retrying generated standby slate, validated encoding profiles, aspect-preserving crop, target settings controls, and normalized encoded audio | Platform-specific reconnect/transcode validation |
| 401–402 | Adaptive YouTube polling, tenant API-key partitioning with per-key limiters, and signed Kick/X/Twitch handlers | Provider quota dashboards and webhook fleet soak |
| 403 | Twitch/Kick/YouTube outbound adapters and loop prevention | Provider bot credential provisioning and end-to-end relay tests |
| 501 | Persistent monthly quotas with Free/Pro/Enterprise hour and destination enforcement, tenant usage snapshots, Plan Usage dashboard card, and generic, Stripe, and LemonSqueezy webhooks | Production account/customer lifecycle integration |
| 502 | Tenant metrics (bitrate, dropped frames, reconnects), Prometheus scrape, dashboard QoS cards, and structured tracing spans | Collector deployment and export configuration |
| 503 | Global and tenant emergency-stop endpoints, administrator-only authorization, and incident runbook | Production authorization review |
