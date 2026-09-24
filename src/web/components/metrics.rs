use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use std::collections::HashMap;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{View, attributes, component, view},
};

fn format_bitrate(bits_per_second: u64) -> String {
    if bits_per_second >= 1_000_000 {
        format!("{:.2} Mbps", bits_per_second as f64 / 1_000_000.0)
    } else if bits_per_second >= 1_000 {
        format!("{:.0} Kbps", bits_per_second as f64 / 1_000.0)
    } else {
        format!("{bits_per_second} bps")
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

#[component]
pub async fn metrics_page(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = app_context(cx);
    let ingest_bps = app.metrics.current_ingest_bps();
    let ingest_bytes = app.metrics.current_ingest_bytes();
    let egress_bytes = app.metrics.current_egress_bytes();
    let total_bytes = ingest_bytes.saturating_add(egress_bytes);
    let chat_messages_received = app.metrics.current_chat_messages_received();
    let current = app
        .metrics
        .current_target_bitrates()
        .into_iter()
        .map(|sample| (sample.name.clone(), sample))
        .collect::<HashMap<_, _>>();
    let targets = app.config.get().targets.clone();

    Ok(view! {
        <section aria-labelledby="ingest-heading">
            <div class="mb-2 flex items-center justify-between gap-4">
                <h2 id="ingest-heading" class="hud-label flex items-center gap-2">
                    <span class="hud-dot bg-signal text-signal"></span>
                    "STREAM INGEST"
                </h2>
                <span data-metrics-status="true" class="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
                    "LIVE"
                </span>
            </div>
            <div
                class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
                data-metrics-charts="true"
            >
                card(
                    attrs: attributes! {
                        class="!gap-2 !py-3"
                        data-ingest-metric="true"
                    },
                    card_header(
                        attrs: attributes! { class="!px-3" },
                        <div class="flex items-center justify-between gap-3">
                            card_title("Streamer ingest")
                            <span class="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">"Inbound"</span>
                        </div>
                    )
                    card_content(
                        attrs: attributes! { class="!px-3" },
                        <div class="mb-2">
                            <div
                                class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                            >
                                "Ingest bitrate"
                            </div>
                            <div
                                class="hud-value text-xl"
                                data-ingest-bitrate="true"
                            >
                                (format_bitrate(ingest_bps))
                            </div>
                        </div>
                        <canvas
                            class="h-28 w-full"
                            height="112"
                            aria-label="Streamer ingest bitrate history"
                        ></canvas>
                    )
                )
                card(
                    attrs: attributes! {
                        class="!gap-2 !py-3"
                        data-transfer-metric="true"
                    },
                    card_header(
                        attrs: attributes! { class="!px-3" },
                        <div class="flex items-center justify-between gap-3">
                            card_title("Data transferred")
                            <span class="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">"In / Out"</span>
                        </div>
                    )
                    card_content(
                        attrs: attributes! { class="!px-3" },
                        <div class="mb-2 grid grid-cols-3 gap-2">
                            <div>
                                <div
                                    class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                                >
                                    "In"
                                </div>
                                <div
                                    class="hud-value text-xl"
                                    data-transfer-in="true"
                                >
                                    (format_bytes(ingest_bytes))
                                </div>
                            </div>
                            <div>
                                <div
                                    class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                                >
                                    "Out"
                                </div>
                                <div
                                    class="hud-value text-xl"
                                    data-transfer-out="true"
                                >
                                    (format_bytes(egress_bytes))
                                </div>
                            </div>
                            <div>
                                <div
                                    class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                                >
                                    "Total"
                                </div>
                                <div
                                    class="hud-value text-xl"
                                    data-transfer-total="true"
                                >
                                    (format_bytes(total_bytes))
                                </div>
                            </div>
                        </div>
                        <canvas
                            class="h-28 w-full"
                            height="112"
                            aria-label="Data transferred history (in, out, total)"
                        ></canvas>
                    )
                )
            </div>
        </section>
        <section aria-labelledby="targets-heading" class="mt-6">
            <div class="mb-2 flex items-center justify-between gap-4">
                <h2 id="targets-heading" class="hud-label flex items-center gap-2">
                    <span class="hud-dot bg-info text-info"></span>
                    "TARGETS"
                </h2>
            </div>
            <div
                class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
                data-metrics-charts="true"
            >
                for target in &targets {
                    let sample = current.get(&target.name);
                    let outbound = sample.map_or(0, |value| value.outbound_bps);
                    card(
                        attrs: attributes! {
                            class="!gap-2 !py-3"
                            data-target-metric=(target.name.clone())
                        },
                        card_header(
                            attrs: attributes! { class="!px-3" },
                            <div class="flex items-center justify-between gap-3">
                                card_title((target.name.clone()))
                                <span class="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                                    (if sample.is_some() { "Relaying" } else { "Idle" })
                                </span>
                            </div>
                        )
                        card_content(
                            attrs: attributes! { class="!px-3" },
                            <div class="mb-2">
                                <div>
                                    <div
                                        class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                                    >
                                        "Outbound bitrate"
                                    </div>
                                    <div
                                        class="hud-value text-xl"
                                        data-bitrate-out="true"
                                    >
                                        (format_bitrate(outbound))
                                    </div>
                                </div>
                            </div>
                            <canvas
                                class="h-28 w-full"
                                height="112"
                                aria-label=(format!("{} bitrate history", target.name))
                            ></canvas>
                        )
                    )
                }
            </div>
            if targets.is_empty() {
                "No targets configured. Add a target to begin collecting throughput metrics."
            }
        </section>
        <section aria-labelledby="chat-messages-heading" class="mt-6">
            <div class="mb-2 flex items-center justify-between gap-4">
                <h2 id="chat-messages-heading" class="hud-label flex items-center gap-2">
                    <span class="hud-dot bg-warn text-warn"></span>
                    "CHAT"
                </h2>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                card(
                    attrs: attributes! {
                        class="!gap-2 !py-3"
                        data-chat-messages-metric="true"
                    },
                    card_header(
                        attrs: attributes! { class="!px-3" },
                        <div class="flex items-center justify-between gap-3">
                            card_title("Messages received")
                            <span class="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                                "All sources"
                            </span>
                        </div>
                    )
                    card_content(
                        attrs: attributes! { class="!px-3" },
                        <div class="mb-2">
                            <div
                                class="text-xs uppercase tracking-[0.18em] text-muted-foreground font-mono"
                            >
                                "Total messages"
                            </div>
                            <div
                                class="hud-value text-xl"
                                data-chat-messages-received="true"
                            >
                                (chat_messages_received.to_string())
                            </div>
                        </div>
                        <canvas
                            class="h-28 w-full"
                            height="112"
                            aria-label="Chat messages received history"
                        ></canvas>
                    )
                )
            </div>
        </section>
    })
}
