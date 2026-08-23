use crate::server::state::AppHandle;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use std::collections::HashMap;
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
pub async fn metrics_page(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    let ingest_bps = app.metrics.current_ingest_bps();
    let current = app
        .metrics
        .current_target_bitrates()
        .into_iter()
        .map(|sample| (sample.name.clone(), sample))
        .collect::<HashMap<_, _>>();
    let targets = app.config.get().targets.clone();

    view! {
        <section aria-labelledby="target-throughput-heading">
            <div class="mb-2 flex items-end justify-between gap-4">
                <div>
                    <h2 id="target-throughput-heading" class="text-base font-semibold">"Throughput"</h2>
                </div>
                <span data-metrics-status="true" class="text-xs text-muted-foreground">"Live"</span>
            </div>
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4" data-metrics-charts="true">
                card(
                    attrs: attributes! { class="!gap-3 !rounded-lg !py-3" data-ingest-metric="true" },
                    card_header(
                        attrs: attributes! { class="!px-3" },
                        <div class="flex items-center justify-between gap-3">
                            card_title("Streamer ingest")
                            <span class="text-xs text-muted-foreground">"Inbound"</span>
                        </div>
                    )
                    card_content(
                        attrs: attributes! { class="!px-3" },
                        <div class="mb-2">
                            <div class="text-xs uppercase tracking-wide text-muted-foreground">"Ingest bitrate"</div>
                            <div class="text-base font-semibold" data-ingest-bitrate="true">(format_bitrate(ingest_bps))</div>
                        </div>
                        <canvas class="h-28 w-full" height="112" aria-label="Streamer ingest bitrate history"></canvas>
                    )
                )
                for target in &targets {
                    let sample = current.get(&target.name);
                    let outbound = sample.map_or(0, |value| value.outbound_bps);
                    card(
                        attrs: attributes! { class="!gap-3 !rounded-lg !py-3" data-target-metric=(target.name.clone()) },
                        card_header(
                            attrs: attributes! { class="!px-3" },
                            <div class="flex items-center justify-between gap-3">
                                card_title((target.name.clone()))
                                <span class="text-xs text-muted-foreground">(if sample.is_some() { "Relaying" } else { "Idle" })</span>
                            </div>
                        )
                        card_content(
                            attrs: attributes! { class="!px-3" },
                            <div class="mb-2">
                                <div>
                                    <div class="text-xs uppercase tracking-wide text-muted-foreground">"Outbound bitrate"</div>
                                    <div class="text-base font-semibold" data-bitrate-out="true">(format_bitrate(outbound))</div>
                                </div>
                            </div>
                            <canvas class="h-28 w-full" height="112" aria-label=(format!("{} bitrate history", target.name))></canvas>
                        )
                    )
                }
            </div>
            if targets.is_empty() {
                "No targets configured. Add a target to begin collecting throughput metrics."
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
