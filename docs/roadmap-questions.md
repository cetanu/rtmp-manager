# Roadmap questions

This file records decisions that require an external provider or deployment
contract. Current code uses the simplest explicit behavior until each question
is answered.

| Goal | Question | Current behavior |
| --- | --- | --- |
| GOAL-402 | How should Twitch/Kick/X provider deliveries identify a tenant when one webhook URL serves many tenants: provider subscription ID, a signed tenant token, or a dedicated URL per tenant? | The endpoint requires `X-Tenant-Stream-Key` and independently verifies the platform signature before enqueueing. |
| GOAL-503 | What thumbnail retention and storage contract should administrators receive (local files, object storage, or a deployment-provided media service)? | Active stream state and emergency controls are available; periodic thumbnails remain a deployment acceptance item. |
