use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{component, view, View},
    Result,
};

#[component]
async fn metric_card(#[into] id: String, #[into] value: String, child: View) -> Result {
    view! {
        card(
            card_content(
                <div class="text-4xl font-bold tracking-tight text-primary mt-2" id=(id)>(value)</div>
                <div class="text-sm font-medium text-muted-foreground mt-2">(child)</div>
            )
        )
    }
}

#[component]
pub async fn metrics_grid(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let active_connections = state.metrics.active_connections.load(Ordering::Relaxed);
    let total_connections = state.metrics.total_connections.load(Ordering::Relaxed);
    let relays = state.active_relays.lock().await;
    let active_streams = relays.len();
    let active_relays: usize = relays.values().map(Vec::len).sum();

    view! {
        <section class="mb-8">
            <div class="flex items-center justify-between mb-3">
                <h2 class="text-sm font-medium text-muted-foreground">"Current activity"</h2>
                <a href="/" class="text-sm font-medium underline underline-offset-4">"Refresh metrics"</a>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
                metric_card(id: "metric_streams", value: active_streams.to_string(), "Active Streams")
                metric_card(id: "metric_relays", value: active_relays.to_string(), "Active Relays")
                metric_card(id: "metric_active_conn", value: active_connections.to_string(), "Active Connections")
                metric_card(id: "metric_total_conn", value: total_connections.to_string(), "Total Connections")
            </div>
        </section>
    }
}
