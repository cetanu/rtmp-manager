use crate::server::state::ProxyState;
use crate::web::components::ui::button::{button_variants, ButtonSize, ButtonVariant};
use crate::web::components::ui::card::{card, card_content, card_footer, card_header, card_title};
use std::sync::Arc;
use topcoat::{
    context::{app_context, Cx},
    runtime::{procedure, shard},
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

#[shard]
async fn chat_inbox_content(cx: &Cx, revision: f64) -> Result {
    let _ = revision;
    let state: &Arc<ProxyState> = app_context(cx);
    let snapshot = state.chat_inbox.lock().await.snapshot()?;
    let youtube_status = state.youtube_status.read().await.clone();

    view! {
        card_content(
            <div class="mb-4 flex justify-end gap-4 text-right">
                <div class="text-sm font-medium">(format!("{} waiting", snapshot.waiting))</div>
                <div class="text-sm text-muted-foreground">(format!("{} dropped", snapshot.dropped))</div>
            </div>
            if let Some(status) = youtube_status {
                <div class="mb-4 rounded-lg border bg-muted/30 px-4 py-3 text-sm">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                        <span class="font-semibold">"YouTube ingest"</span>
                        <span class="uppercase tracking-wide text-muted-foreground">(status.state)</span>
                    </div>
                    <p class="mt-1 text-muted-foreground">(status.detail)</p>
                    <p class="mt-1 text-xs text-muted-foreground">
                        (format!("{} messages received", status.messages_received))
                    </p>
                </div>
            }
            if let Some(current) = snapshot.current {
                <div class="flex flex-col gap-4 rounded-xl border bg-surface p-5">
                    <div class="flex items-center justify-between gap-4">
                        <div class="min-w-0">
                            <div class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                                (current.source)
                            </div>
                            <div class="truncate font-semibold">(current.author)</div>
                        </div>
                        <div class="shrink-0 text-xs text-muted-foreground">
                            (current.sent_at.unwrap_or_default())
                        </div>
                    </div>
                    <p class="whitespace-pre-wrap break-words text-lg leading-relaxed">
                        (current.text)
                    </p>
                </div>
            } else {
                <div class="rounded-xl border border-dashed p-8 text-center text-muted-foreground">
                    "No chat messages waiting."
                </div>
            }
        )
    }
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
                <div class="flex flex-col gap-1">
                    card_title("Chat Inbox")
                    <p class="text-sm text-muted-foreground">"One message at a time from every connected chat source."</p>
                </div>
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
