## Milestone 2: Multi-Tenancy & Data Security

### GOAL-201: Dynamic Per-Tenant Ingest Stream Key Authentication
- **Objective:** Authenticate incoming RTMP/SRT streams against a tenant database instead of a single global stream key.
- **Current State:** [`ProxyHandler::on_publish`](src/server/handler.rs#L32-L52) compares `params.stream_key` against single static `app.config.get().server.ingest_stream_key`.
- **Technical Requirements:**
  1. Update stream key validation to query database for active tenant matching the stream key (or hashed stream key).
  2. Associate the incoming RTMP/SRT session context with `tenant_id`.
  3. Ensure downstream relay dispatching ([`src/server/stream_actor.rs`](src/server/stream_actor.rs)) loads and applies only that specific tenant's destination targets.
  4. Enforce concurrency limits (e.g., reject duplicate active streams for the same tenant unless multi-stream is permitted).
- **Acceptance Criteria:**
  - Two distinct users streaming with different stream keys publish concurrently to their respective configured destinations without crosstalk.

---

### GOAL-202: Multi-User Authentication & RBAC
- **Objective:** Add user registration, login, session management, and role-based access control (Admin, User) for the dashboard and REST APIs.
- **Current State:** Single-user dashboard without login sessions or authentication middleware.
- **Technical Requirements:**
  1. Implement password hashing using Argon2id and session management (signed HTTP-only cookies or JWTs).
  2. Support OAuth2 login providers (Twitch, Google, Discord, GitHub).
  3. Add authentication middleware to protected web routes and API endpoints.
  4. Implement user profile management (reset stream key, change email/password, revoke active sessions).
- **Acceptance Criteria:**
  - Users can register, log in, manage only their personal destinations/chat integrations, and reset their private ingest key.

---

### GOAL-203: Envelope Encryption for Destination Stream Keys & OAuth Tokens
- **Objective:** Encrypt third-party stream keys and OAuth refresh tokens at rest in the database.
- **Current State:** Target stream keys and API secrets are stored as plaintext in SQLite ([`src/config.rs`](src/config.rs)).
- **Technical Requirements:**
  1. Implement AES-256-GCM envelope encryption for sensitive fields in the database schema.
  2. Support master key derivation via environment variable (`MASTER_ENCRYPTION_KEY`) or cloud KMS (AWS KMS / GCP KMS / HashiCorp Vault).
  3. Ensure logs continue to redact destination stream keys and tokens ([`src/util.rs:redact_secrets`](src/util.rs)).
- **Acceptance Criteria:**
  - Plain database dumps show ciphertexts for stream keys and OAuth tokens; keys are decrypted only in-memory when launching relays or refreshing tokens.

---

### GOAL-204: Database Abstraction & PostgreSQL Migration Support
- **Objective:** Provide support for PostgreSQL in multi-node/SaaS deployments while retaining SQLite for single-node self-hosters.
- **Current State:** SQLite is coupled directly via `rusqlite` / `toasty` ([`src/config.rs`](src/config.rs), [`src/chat.rs`](src/chat.rs)).
- **Technical Requirements:**
  1. Define database traits/repository layers or utilize an async SQL toolkit (e.g., `sqlx` or `sea-orm`) supporting both SQLite and PostgreSQL drivers.
  2. Create versioned database migrations for both backends.
  3. Include connection pooling, connection health checks, and automatic migration on startup.
- **Acceptance Criteria:**
  - Setting `DATABASE_URL=postgres://...` runs the system seamlessly against PostgreSQL; setting `DATABASE_URL=sqlite://...` runs with embedded SQLite.

---

## Milestone 3: Media Plane Scaling & Relay Resilience

### GOAL-301: Decouple Control Plane from Media Relay Workers
- **Objective:** Separate the Web/API server from the edge ingest and relay nodes using an async message broker.
- **Current State:** The web process directly launches and manages FFmpeg child processes locally ([`src/server/relay.rs`](src/server/relay.rs)).
- **Technical Requirements:**
  1. Create a distributed task model using Redis Streams, NATS, or RabbitMQ for relay orchestration.
  2. Edge nodes handle RTMP/SRT ingestion, report session start to the Control Plane, and receive target broadcast instructions.
  3. Control Plane coordinates stream lifecycle, updates user status in real-time, and dispatches scale commands.
- **Acceptance Criteria:**
  - The Web dashboard can be restarted or scaled across multiple nodes without dropping or interrupting active live streams handled by media workers.

---

### GOAL-302: Sandboxed Relay Supervision & Resource Capping
- **Objective:** Supervise relay processes with CPU, memory, and timeout constraints to prevent host exhaustion.
- **Current State:** `tokio::process::Command::new("ffmpeg")` runs directly on the host without cgroup limits.
- **Technical Requirements:**
  1. Wrap relay execution in worker-level process supervisors with explicit limits (max memory, max CPU time, max I/O buffer).
  2. Implement zombie process cleanup and heartbeat monitoring.
  3. Capture structured FFmpeg progress events (bitrate, fps, dropped frames, speed) and expose them via real-time WebSocket metrics.
- **Acceptance Criteria:**
  - An unresponsive or hung FFmpeg relay is killed and restarted within 5 seconds without affecting other concurrent streams on the host.

---

### GOAL-303: Stream Disconnect Grace Period & Standby Slate Loop
- **Objective:** Prevent downstream platforms (Twitch, YouTube) from terminating a broadcast when the streamer experiences a brief connection hiccup.
- **Current State:** Ingest disconnect triggers immediate `on_unpublish` which terminates downstream relays ([`src/server/handler.rs:on_unpublish`](src/server/handler.rs#L54-L57)).
- **Technical Requirements:**
  1. On ingest disconnect, hold downstream relay pipelines open for a configurable grace period (e.g., 30–60 seconds).
  2. Switch the active output feed to a looping standby video slate ("Stream Reconnecting...") or black screen with silence.
  3. Seamlessly splice the incoming live video back into the running relay when the streamer reconnects within the grace window.
- **Acceptance Criteria:**
  - Disconnecting OBS for 15 seconds and reconnecting resumes the stream on Twitch and YouTube without ending the broadcast session.

---

### GOAL-304: Hardware-Accelerated Transcoding & Encoding Profiles
- **Objective:** Support automatic transcoding and format adaptation (e.g. 9:16 vertical crop, audio normalization, bitrate capping).
- **Current State:** Relays strictly pass through incoming bitstreams using `-c copy` ([`src/server/relay.rs:159`](src/server/relay.rs#L159)).
- **Technical Requirements:**
  1. Support FFmpeg hardware acceleration flags (`h264_nvenc`, `h264_vaapi`, `h264_qsv`, `videotoolbox`).
  2. Provide destination encoding profiles:
     - Bitrate cap (e.g., transcode 1440p 15Mbps down to 1080p60 6Mbps for Twitch).
     - Vertical crop / aspect ratio filter (e.g., 1080x1920 for TikTok/Instagram).
     - Audio resampling (e.g., downmixing surround sound to AAC 128k stereo).
  3. Allow users to select "Passthrough (-c copy)" or specific destination profiles.
- **Acceptance Criteria:**
  - A user streaming at 1440p can multiplex to YouTube with `-c copy` and simultaneously to Twitch transcoded to 1080p60 6000kbps via NVENC/CPU encoding.

---

## Milestone 4: Scalable Chat Ingestion & Bot Relaying

### GOAL-401: Distributed Quota-Aware YouTube Chat Polling Worker
- **Objective:** Prevent YouTube Data API quota exhaustion across multiple concurrent streamers.
- **Current State:** Individual polling loops run per active stream ([`src/chat/youtube.rs`](src/chat/youtube.rs)), consuming default API quota units rapidly.
- **Technical Requirements:**
  1. Implement dynamic polling backoff based on stream viewer activity and YouTube's `pollingIntervalMillis` response header.
  2. Support custom user-provided Google Cloud API credentials or OAuth user tokens to partition quota usage per user.
  3. Add centralized quota tracking and circuit-breaking when quota budgets approach limits.
- **Acceptance Criteria:**
  - 50 concurrent YouTube chat streams operate simultaneously without hitting Google API 429 / quota exceeded errors.

---

### GOAL-402: Webhook Multiplexing for Kick & Twitch EventSub
- **Objective:** Receive platform webhooks at a central edge and route verified events to appropriate tenant chat queues.
- **Current State:** Kick webhooks are processed on a single local broadcast channel ([`src/chat/kick.rs`](src/chat/kick.rs)).
- **Technical Requirements:**
  1. Implement a unified webhook dispatcher at `/api/v1/webhooks/:platform`.
  2. Verify HMAC signatures per tenant (using tenant-specific secret keys).
  3. Parse event payloads (Twitch EventSub, Kick Chat Webhooks) and dispatch to the tenant's real-time chat queue via message broker.
- **Acceptance Criteria:**
  - Webhooks from multiple streamers on Twitch and Kick are verified and routed to the correct streamer's dashboard with zero leakage across tenants.

---

### GOAL-403: Bidirectional Cross-Platform Chat Relaying
- **Objective:** Allow streamers to mirror chat messages across platforms (e.g. reposting YouTube comments into Twitch and Kick chats).
- **Current State:** Read-only ingestion and aggregation of chat messages into a local display queue ([`src/chat.rs`](src/chat.rs)).
- **Technical Requirements:**
  1. Implement authenticated bot client dispatchers for Twitch (IRC / Helix API), Kick, and YouTube Live Chat API.
  2. Add user configuration for relay rules (e.g., "[YouTube] {author}: {message}" -> send to Twitch).
  3. Add bot moderation filters, loopback prevention (ignore bot's own relayed messages), and spam rate limiters.
- **Acceptance Criteria:**
  - When enabled, a message sent in YouTube chat is mirrored to Twitch chat within 1 second with author attribution and loop prevention.

---

## Milestone 5: SaaS Commercialization & Observability

### GOAL-501: Subscription Billing & Usage Quota Enforcement
- **Objective:** Integrate subscription billing (Stripe / LemonSqueezy) and enforce plan limits.
- **Current State:** No billing or usage limits.
- **Technical Requirements:**
  1. Integrate Stripe Customer Portal and webhook events (`checkout.session.completed`, `customer.subscription.updated/deleted`).
  2. Implement plan tiers with explicit feature gates:
     - Free: 2 simultaneous destinations, 720p, watermark optional, 20 hrs/month.
     - Pro: 5 destinations, 1080p60, transcode profiles, unlimited hours.
     - Enterprise: Custom RTMP destinations, SRT ingest, dedicated edge nodes.
  3. Enforce real-time stream cutoffs or warnings when monthly bandwidth or concurrent destination limits are exceeded.
- **Acceptance Criteria:**
  - A user on the Free tier attempting to enable a 3rd destination is prompted to upgrade, and Stripe webhook upgrades unlock features immediately.

---

### GOAL-502: Per-Tenant Metrics & OpenTelemetry Instrumentation
- **Objective:** Track stream health, bitrate stability, network egress, and application telemetry per tenant.
- **Current State:** Basic aggregate counters in [`src/metrics.rs`](src/metrics.rs).
- **Technical Requirements:**
  1. Instrument codebase with OpenTelemetry spans and Prometheus metrics labeled by `tenant_id`, `destination_id`, and `codec`.
  2. Track key QoS metrics: Ingest Bitrate, Egress Bitrate, Relay Latency, Dropped Frames, Reconnection Count.
  3. Expose real-time stream health graphs on the user's dashboard.
- **Acceptance Criteria:**
  - Streamers see live charts of incoming bitrate and destination FPS; platform admins have Prometheus/Grafana dashboards showing total network egress per tenant.

---

### GOAL-503: Live Stream Moderation & Emergency Killswitch
- **Objective:** Provide automated abuse detection and admin controls to terminate rogue or abusive streams.
- **Current State:** No admin moderation or remote killswitch capabilities.
- **Technical Requirements:**
  1. Admin dashboard to view all currently active streams across all edge workers.
  2. One-click instant stream termination and stream key revocation.
  3. Integration with thumbnail snapshot workers (e.g., periodic FFmpeg 1-frame snapshot every 60s) for administrative visual auditing.
  4. Audit logging of all administrative actions.
- **Acceptance Criteria:**
  - Platform administrators can view active stream thumbnails and immediately terminate an abusive stream across all edge workers within 500ms.
