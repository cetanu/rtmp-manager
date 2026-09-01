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
