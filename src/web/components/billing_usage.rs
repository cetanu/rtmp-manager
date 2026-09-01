use crate::server::state::AppHandle;
use crate::web::auth;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{component, view},
};

#[component]
pub async fn billing_usage(cx: &Cx) -> Result {
    let app: &AppHandle = app_context(cx);
    let tenant_id = auth::current_user(cx).tenant_id.as_str().to_owned();
    let usage = app
        .usage
        .current_usage(&tenant_id, crate::util::now_unix_secs() as i64)
        .await?;
    let consumed_hours = usage.stream_seconds / 3600;
    let limit = usage
        .limit_seconds
        .map(|seconds| format!("{} hours", seconds / 3600))
        .unwrap_or_else(|| "Unlimited".to_owned());

    view! {
        card(
            card_header(card_title("Plan Usage"))
            card_content(
                <div class="grid gap-3 sm:grid-cols-3">
                    <div>
                        <div class="text-xs uppercase tracking-wide text-muted-foreground">"Plan"</div>
                        <div class="font-semibold capitalize">(usage.plan)</div>
                    </div>
                    <div>
                        <div class="text-xs uppercase tracking-wide text-muted-foreground">"Stream time"</div>
                        <div class="font-semibold">(format!("{consumed_hours} hours of {limit}"))</div>
                    </div>
                    <div>
                        <div class="text-xs uppercase tracking-wide text-muted-foreground">"Active streams"</div>
                        <div class="font-semibold">(usage.active_streams)</div>
                    </div>
                </div>
            )
        )
    }
}
