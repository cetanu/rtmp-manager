use crate::server::state::ProxyState;
use crate::web::components::chat_inbox_content::chat_inbox_content;
use crate::web::components::ui::button::{button_variants, ButtonSize, ButtonVariant};
use crate::web::components::ui::card::{
    card, card_description, card_footer, card_header, card_title,
};
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    runtime::procedure,
    view::{attributes, component, view},
    Result,
};

#[procedure]
async fn acknowledge_chat(cx: &Cx, displayed_id: String) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    let mut inbox = state.chat_inbox.lock().await;
    if let Ok(displayed_id) = displayed_id.parse() {
        if inbox.acknowledge(displayed_id)? {
            state.notify_chat_changed();
        }
    }
    Ok(current_message_id(&inbox.snapshot()?))
}

#[procedure]
async fn refresh_chat(cx: &Cx) -> Result<String> {
    let state: &Arc<ProxyState> = app_context(cx);
    Ok(current_message_id(
        &state.chat_inbox.lock().await.snapshot()?,
    ))
}

fn current_message_id(snapshot: &crate::chat::ChatInboxSnapshot) -> String {
    snapshot
        .current
        .as_ref()
        .map(|message| message.id.to_string())
        .unwrap_or_default()
}

#[component]
pub async fn chat_inbox(cx: &Cx) -> Result {
    let state: &Arc<ProxyState> = app_context(cx);
    let initial_id = current_message_id(&state.chat_inbox.lock().await.snapshot()?);
    let outline_button = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary_button = button_variants(ButtonVariant::Primary, ButtonSize::Md);

    view! {
        signal current_id = initial_id;
        signal revision = 0.0;

        card(
            attrs: attributes! { class="mb-8" },
            card_header(
                card_title("Chat Inbox")
                card_description("One message at a time from every connected chat source.")
            )
            chat_inbox_content(revision: $(revision.get()))
            card_footer(
                attrs: attributes! { class="justify-end" },
                <button
                    id="chat-refresh-button"
                    type="button"
                    class=(outline_button)
                    @click=$(async |_event| {
                        let refreshed_id = refresh_chat().await;
                        current_id.set(refreshed_id);
                        revision.set(revision.get() + 1.0);
                    })
                >
                    "Check for messages"
                </button>
                <button
                    type="button"
                    class=(primary_button)
                    :disabled=$(current_id.get().is_empty())
                    @click=$(async |_event| {
                        let next_id = acknowledge_chat(current_id.get()).await;
                        current_id.set(next_id);
                        revision.set(revision.get() + 1.0);
                    })
                >
                    "Acknowledge message"
                </button>
            )
        )
    }
}
