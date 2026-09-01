# Live-stream incident runbook

## GOAL-503 emergency stop

Use the global endpoint only when all active broadcasts must stop:

```text
POST /api/admin/emergency-stop
```

Use the tenant-scoped endpoint when isolating one account:

```text
POST /api/admin/tenants/{tenant_id}/emergency-stop
```

Both endpoints require an authenticated administrator session. A tenant user
cannot stop another tenant's streams; the tenant-scoped route also checks the
requested tenant identifier before dispatching the stop command.

After stopping a stream:

1. Confirm the dashboard status is no longer `live`.
2. Confirm target relays have disconnected and no FFmpeg worker remains for
   the affected tenant.
3. Review the tenant's usage record; shutdown paths account for elapsed
   stream time before removing active reservations.
4. Restore service only after the provider incident or abusive stream has
   been understood and the tenant is cleared to publish again.

Record the operator, UTC timestamp, tenant scope, reason, and verification
steps in the incident ticket. The endpoints intentionally do not expose
stream keys or provider credentials in responses or logs.
