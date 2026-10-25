use crate::server::state::AppHandle;
use crate::web::components::ui::floating::FloatingAlign;
use crate::web::components::ui::popover::{popover, popover_content, popover_trigger};
use topcoat::{
    Result,
    context::{Cx, app_context},
    runtime::{connected, shard},
    view::{View, attributes, component, element_id, emit, live, view},
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
                        <th class="px-4 py-3 font-medium">"Received"</th>
                        <th class="px-4 py-3 font-medium">"Platform"</th>
                        <th class="px-4 py-3 font-medium">"Content type"</th>
                        <th class="px-4 py-3 font-medium">"Size"</th>
                        <th class="px-4 py-3 font-medium">"Payload"</th>
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
                            let payload_id = element_id(cx);
                            <tr>
                                <td class="whitespace-nowrap px-4 py-3 font-mono text-xs">
                                    if let Some(dt) = crate::util::to_localized_datetime(
                                        entry.timestamp_ms,
                                    ) {
                                        (dt)
                                    } else {
                                        (entry.timestamp_ms.to_string())
                                    }
                                </td>
                                <td class="px-4 py-3 font-medium">(&entry.platform)</td>
                                <td class="px-4 py-3 text-xs text-muted-foreground">
                                    (entry.content_type.unwrap_or_else(|| "—".into()))
                                </td>
                                <td class="whitespace-nowrap px-4 py-3 font-mono text-xs">
                                    (format!("{} B", entry.body_bytes))
                                </td>
                                <td class="px-4 py-3">
                                    popover(
                                        popover_trigger(
                                            target: &payload_id,
                                            attrs: attributes! {
                                                class="inline-flex items-center gap-1.5 rounded border border-border bg-white/5 px-2.5 py-1 font-mono text-xs text-muted-foreground transition-colors hover:bg-white/10 hover:text-foreground"
                                            },
                                            "View Payload"
                                        )
                                        popover_content(
                                            id: &payload_id,
                                            align: FloatingAlign::End,
                                            attrs: attributes! { class="!w-[34rem] max-w-[92vw] p-4 text-xs" },
                                            <div class="flex flex-col gap-2">
                                                <div
                                                    class="flex items-center justify-between border-b border-border pb-2"
                                                >
                                                    <span class="font-mono font-semibold text-foreground">
                                                        (format!("{} Webhook Payload", entry.platform))
                                                    </span>
                                                    <span class="font-mono text-xs text-muted-foreground">
                                                        (format!("{} bytes", entry.body_bytes))
                                                    </span>
                                                </div>
                                                <pre
                                                    class="max-h-80 overflow-auto rounded bg-black/50 p-2.5 font-mono text-[11px] whitespace-pre-wrap break-all text-emerald-200/90 selection:bg-emerald-800"
                                                >
                                                    (entry.payload)
                                                </pre>
                                            </div>
                                        )
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
