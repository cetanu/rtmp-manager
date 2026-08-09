use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::empty_state::empty_state;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{attributes, component, view},
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

#[component]
async fn metric_card(#[into] id: String, #[into] value: String, #[into] label: String) -> Result {
    view! {
        card(card_content(
            <div class="mt-2 text-4xl font-bold tracking-tight text-primary" id=(id)>(value)</div>
            <div class="mt-2 text-sm font-medium text-muted-foreground">(label)</div>
        ))
    }
}

#[component]
pub async fn metrics_page(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let active_connections = state.metrics.active_connections.load(Ordering::Relaxed);
    let total_connections = state.metrics.total_connections.load(Ordering::Relaxed);
    let ingest_bps = state.metrics.current_ingest_bps();
    let active_streams = usize::from(state.stream_status().await.active);
    let active_relays: usize = state
        .active_relays
        .lock()
        .await
        .values()
        .flat_map(|relays| relays.iter())
        .filter(|relay| relay.is_running())
        .count();
    let current = state
        .metrics
        .current_target_bitrates()
        .into_iter()
        .map(|sample| (sample.name.clone(), sample))
        .collect::<HashMap<_, _>>();
    let targets = state.config.read().await.targets.clone();

    view! {
        <section class="mb-8">
            <div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
                metric_card(id: "metric_streams", value: active_streams.to_string(), label: "Active streams")
                metric_card(id: "metric_relays", value: active_relays.to_string(), label: "Active relays")
                metric_card(id: "metric_active_conn", value: active_connections.to_string(), label: "Active connections")
                metric_card(id: "metric_total_conn", value: total_connections.to_string(), label: "Total connections")
            </div>
        </section>
        <section class="mb-8">
            card(
                attrs: attributes! { data-ingest-metric="true" },
                card_header(
                    <div class="flex items-center justify-between gap-3">
                        card_title("Streamer ingest")
                        <span class="text-xs text-muted-foreground">"Inbound"</span>
                    </div>
                )
                card_content(
                    <div class="mb-4">
                        <div class="text-xs uppercase tracking-wide text-muted-foreground">"Ingest bitrate"</div>
                        <div class="mt-1 text-xl font-semibold" data-ingest-bitrate="true">(format_bitrate(ingest_bps))</div>
                    </div>
                    <canvas class="h-52 w-full" height="208" aria-label="Streamer ingest bitrate history"></canvas>
                )
            )
        </section>
        <section aria-labelledby="target-throughput-heading">
            <div class="mb-3 flex items-end justify-between gap-4">
                <div>
                    <h2 id="target-throughput-heading" class="text-lg font-semibold">"Throughput"</h2>
                </div>
                <span data-metrics-status="true" class="text-xs text-muted-foreground">"Live"</span>
            </div>
            <div class="grid gap-6 lg:grid-cols-2" data-metrics-charts="true">
                for target in &targets {
                    let sample = current.get(&target.name);
                    let outbound = sample.map_or(0, |value| value.outbound_bps);
                    card(
                        attrs: attributes! { data-target-metric=(target.name.clone()) },
                        card_header(
                            <div class="flex items-center justify-between gap-3">
                                card_title((target.name.clone()))
                                <span class="text-xs text-muted-foreground">(if sample.is_some() { "Relaying" } else { "Idle" })</span>
                            </div>
                        )
                        card_content(
                            <div class="mb-4">
                                <div>
                                    <div class="text-xs uppercase tracking-wide text-muted-foreground">"Outbound bitrate"</div>
                                    <div class="mt-1 text-xl font-semibold" data-bitrate-out="true">(format_bitrate(outbound))</div>
                                </div>
                            </div>
                            <canvas class="h-52 w-full" height="208" aria-label=(format!("{} bitrate history", target.name))></canvas>
                        )
                    )
                }
            </div>
            if targets.is_empty() {
                empty_state("No targets configured. Add a target to begin collecting throughput metrics.")
            }
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::format_bitrate;

    #[test]
    fn formats_bitrate_for_dashboard() {
        assert_eq!(format_bitrate(2_450_000), "2.45 Mbps");
        assert_eq!(format_bitrate(450_000), "450 Kbps");
    }
}
