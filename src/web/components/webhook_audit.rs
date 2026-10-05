use crate::server::state::AppHandle;
use crate::web::components::ui::form::secret_input;
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{connected, shard},
    view::{View, attributes, component, emit, live, view},
};

#[component]
pub async fn webhook_audit() -> Result<impl View> {
    Ok(view! {
        <section class="mt-4" aria-labelledby="webhook-audit-heading">
            <div class="mb-2 flex items-center justify-between gap-4">
                <div>
                    <h2 id="webhook-audit-heading" class="text-base font-semibold">
                        "Webhook audit"
                    </h2>
                    <p class="text-xs text-muted-foreground">
                        "Latest 10 POST requests. Payloads are concealed by default."
                    </p>
                </div>
            </div>
            webhook_audit_table()
        </section>
    })
}

#[shard]
async fn webhook_audit_table(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let app: &AppHandle = app_context(cx);
        let mut changed = app.webhook_audit.subscribe();
        loop {
            let token = emit! { audit_entries() }?;
            if !connected(cx) || changed.changed().await.is_err() {
                break Ok(token);
            }
        }
    })
}

#[component]
async fn audit_entries(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let entries = app.webhook_audit.snapshot();

    Ok(view! {
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
                        <tr>
                            <td
                                colspan="5"
                                class="px-4 py-6 text-center text-muted-foreground"
                            >
                                "No webhooks received since startup."
                            </td>
                        </tr>
                    } else {
                        #[key(entry.id)]
                        for entry in entries {
                            <tr>
                                <td class="whitespace-nowrap px-4 py-3 font-mono text-xs">
                                    (entry.timestamp_ms.to_string())
                                </td>
                                <td class="px-4 py-3">(entry.platform)</td>
                                <td class="px-4 py-3 text-xs text-muted-foreground">
                                    (entry.content_type.unwrap_or_else(|| "—".into()))
                                </td>
                                <td class="whitespace-nowrap px-4 py-3">
                                    (format!("{} B", entry.body_bytes))
                                </td>
                                <td class="px-4 py-3">
                                    secret_input(
                                        control_id: format!("webhook-payload-{}", entry.id),
                                        attrs: attributes! {
                                            value=(entry.payload.chars().take(2048).collect::<String>())
                                            readonly="readonly"
                                            aria-label="Webhook payload (truncated to 2K)"
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
    })
}
