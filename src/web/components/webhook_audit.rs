use crate::server::state::AppHandle;
use crate::web::components::ui::form::secret_input;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{procedure, shard},
    view::{attributes, component, view},
};

#[procedure]
async fn refresh_webhook_audit(cx: &Cx) -> Result<f64> {
    let _app: &AppHandle = app_context(cx);
    Ok(1.0)
}

#[component]
pub async fn webhook_audit(cx: &Cx) -> Result {
    let _app: &AppHandle = app_context(cx);
    view! {
        signal revision = 0.0;
        <section class="mt-4" aria-labelledby="webhook-audit-heading">
            <div class="mb-2 flex items-center justify-between gap-4">
                <div>
                    <h2 id="webhook-audit-heading" class="text-base font-semibold">"Webhook audit"</h2>
                    <p class="text-xs text-muted-foreground">"Latest 10 POST requests. Payloads are concealed by default."</p>
                </div>
                <button
                    type="button"
                    class="inline-flex h-9 items-center justify-center rounded-md border border-border bg-background px-3 text-sm font-medium shadow-xs hover:bg-accent"
                    @click=$(async |_event| {
                        revision.set(revision.get() + refresh_webhook_audit().await);
                    })
                >"Refresh"</button>
            </div>
            webhook_audit_table(revision: $(revision.get()))
        </section>
    }
}

#[shard]
async fn webhook_audit_table(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let app: &AppHandle = app_context(cx);
    let entries = app.webhook_audit.snapshot();

    view! {
        <div class="overflow-x-auto rounded-xl border border-border bg-card shadow-sm">
            <table class="w-full text-left text-sm">
                <thead class="border-b border-border text-xs text-muted-foreground">
                    <tr>
                        <th class="px-4 py-3 font-medium">"Received (Unix ms)"</th>
                        <th class="px-4 py-3 font-medium">"Platform"</th>
                        <th class="px-4 py-3 font-medium">"Content type"</th>
                        <th class="px-4 py-3 font-medium">"Size"</th>
                        <th class="min-w-80 px-4 py-3 font-medium">"Payload"</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-border">
                    if entries.is_empty() {
                        <tr><td colspan="5" class="px-4 py-6 text-center text-muted-foreground">"No webhooks received since startup."</td></tr>
                    } else {
                        for entry in entries {
                            <tr>
                                <td class="whitespace-nowrap px-4 py-3 font-mono text-xs">(entry.timestamp_ms.to_string())</td>
                                <td class="px-4 py-3">(entry.platform)</td>
                                <td class="px-4 py-3 text-xs text-muted-foreground">(entry.content_type.unwrap_or_else(|| "—".into()))</td>
                                <td class="whitespace-nowrap px-4 py-3">(format!("{} B", entry.body_bytes))</td>
                                <td class="px-4 py-3">
                                    secret_input(
                                        control_id: format!("webhook-payload-{}", entry.id),
                                        attrs: attributes! {
                                            value=(entry.payload)
                                            readonly="readonly"
                                            aria-label="Webhook payload"
                                            class="font-mono text-xs"
                                        }
                                    )
                                </td>
                            </tr>
                        }
                    }
                </tbody>
            </table>
        </div>
    }
}
