use crate::server::state::ProxyState;
use crate::web::components::ui::card::{card, card_content, card_header, card_title};
use crate::web::components::ui::input::input;
use crate::web::components::ui::label::label;
use crate::web::components::ui::switch::switch;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn notifications(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let config = state.config.read().await;
    view! {
        card(
            attrs: attributes! { class="mb-8" },
            card_header(
                card_title("Notifications")
            )
            card_content(
                <div class="flex flex-col gap-6">
                    <div class="flex flex-col gap-2">
                        label(attrs: attributes! { for="live_message" }, "Live Message (Sent when stream starts)")
                        <textarea
                            id="live_message"
                            name="notifications[live_message]"
                            placeholder="Stream is LIVE!"
                            class="min-h-[80px] w-full rounded-lg border border-border bg-background px-3 py-2 text-sm shadow-xs transition-colors outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 resize-y"
                        >(config.notifications.live_message.clone())</textarea>
                    </div>

                    <div class="flex flex-col md:flex-row gap-6">
                        <div class="flex-1 flex flex-col gap-2">
                            label(attrs: attributes! { for="discord_webhook" }, "Discord Webhook URL (Optional)")
                            input(attrs: attributes! {
                                type="password"
                                id="discord_webhook"
                                name="notifications[discord_webhook]"
                                value=""
                                placeholder=(if config.notifications.discord_webhook.is_some() { "Configured — leave blank to keep it" } else { "https://discord.com/api/webhooks/..." })
                            })
                            <div class="flex items-center gap-2 mt-1">
                                switch(attrs: attributes! {
                                    id="clear_discord_webhook"
                                    name="notifications[clear_discord_webhook]"
                                    value="true"
                                })
                                label(attrs: attributes! { for="clear_discord_webhook" class="text-muted-foreground" }, "Clear configured Discord webhook")
                            </div>
                        </div>
                        <div class="flex-1 flex flex-col gap-2">
                            label(attrs: attributes! { for="generic_webhook" }, "Generic Webhook URL (Optional)")
                            input(attrs: attributes! {
                                type="password"
                                id="generic_webhook"
                                name="notifications[webhook_url]"
                                value=""
                                placeholder=(if config.notifications.webhook_url.is_some() { "Configured — leave blank to keep it" } else { "https://api.example.com/webhook" })
                            })
                            <div class="flex items-center gap-2 mt-1">
                                switch(attrs: attributes! {
                                    id="clear_generic_webhook"
                                    name="notifications[clear_webhook_url]"
                                    value="true"
                                })
                                label(attrs: attributes! { for="clear_generic_webhook" class="text-muted-foreground" }, "Clear configured generic webhook")
                            </div>
                        </div>
                    </div>
                </div>
            )
        )
    }
}
