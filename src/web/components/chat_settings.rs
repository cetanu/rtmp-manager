use crate::server::state::ProxyState;
use crate::web::components::chat_ingest_fields::chat_ingest_fields;
use crate::web::components::ui::card::{
    card, card_content, card_description, card_header, card_title,
};
use crate::web::components::youtube_chat_fields::youtube_chat_fields;
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn chat_settings(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let chat = state.config.read().await.chat.clone();
    view! {
        card(
            attrs: attributes! { class="h-full" },
            card_header(
                card_title("Chat Ingest")
                card_description("Changes are persisted to SQLite and applied without restarting the server.")
            )
            card_content(
                <div class="flex flex-col gap-6">
                    chat_ingest_fields(chat: &chat)
                    youtube_chat_fields(chat: &chat)
                </div>
            )
        )
    }
}
