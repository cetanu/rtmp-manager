use crate::web::components::ui::{
    button::{ButtonSize, ButtonVariant, button_variants},
    card::{card, card_content, card_header, card_title},
    confirm_delete::confirm_delete,
};
use rtmp_core::chat::automations::AutomationDefinition;
use rtmp_core::server::{preview::StreamState, state::AppHandle};
use topcoat::{
    Result,
    context::{Cx, context},
    runtime::{Event, connected, procedure, shard, signal},
    view::{View, attributes, component, element_id, emit, live, view},
};

#[procedure]
async fn save_automation(
    cx: &Cx,
    id: Option<u64>,
    name: String,
    pattern: String,
    script: String,
    enabled: bool,
    consume: bool,
) -> Result<String> {
    let app: &AppHandle = context(cx);
    Ok(app
        .chat
        .save_automation(
            id,
            AutomationDefinition {
                name,
                pattern,
                script,
                enabled,
                consume,
            },
        )
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default())
}

#[procedure]
async fn test_automation(cx: &Cx, pattern: String, script: String, text: String) -> Result<String> {
    let app: &AppHandle = context(cx);
    let result = tokio::task::spawn_blocking(move || {
        let definition = AutomationDefinition {
            name: "Test".into(),
            pattern,
            script,
            enabled: false,
            consume: false,
        };
        let message = rtmp_core::chat::IncomingChatMessage {
            source: rtmp_core::chat::Source::Twitch,
            external_id: "test-message".into(),
            author: "test_viewer".into(),
            user_id: Some("test-viewer".into()),
            text,
            parts: vec![],
            avatar_url: None,
            sent_at: None,
        };
        rtmp_core::chat::automations::runtime::test_message(&definition, &message)
    })
    .await;
    Ok(match result {
        Ok(Ok(Some(result))) => {
            let preview_status = if !result.alerts.is_empty() {
                match app
                    .chat
                    .preview_alerts(app.config.database(), result.alerts.clone())
                    .await
                {
                    Ok(0) => {
                        "\n\nNo overlay connected; open the OBS overlay before testing.".to_owned()
                    }
                    Ok(count) => format!("\n\nTest alerts sent to {count} connected overlay(s)."),
                    Err(error) => format!("\n\nOverlay preview error: {error}"),
                }
            } else {
                String::new()
            };
            format!(
                "Output: {}\n\nGlobal state: {}\nViewer state: {}\n\nAlerts: {}{}",
                if result.output.is_empty() {
                    "(no output)"
                } else {
                    &result.output
                },
                serde_json::to_string_pretty(&result.state).unwrap_or_default(),
                serde_json::to_string_pretty(&result.viewer).unwrap_or_default(),
                serde_json::to_string(&result.alerts).unwrap_or_default(),
                preview_status,
            )
        }
        Ok(Ok(None)) => "Not matched.".into(),
        Ok(Err(error)) => format!("Error: {error}"),
        Err(error) => format!("Error: {error}"),
    })
}

#[procedure]
async fn toggle_automation(cx: &Cx, id: u64, enabled: bool) -> Result<String> {
    let app: &AppHandle = context(cx);
    Ok(app
        .chat
        .toggle_automation(id, enabled)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default())
}

#[procedure]
async fn delete_automation(cx: &Cx, id: u64) -> Result<String> {
    let app: &AppHandle = context(cx);
    Ok(app
        .chat
        .delete_automation(id)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default())
}

#[component]
async fn automation_editor(
    cx: &Cx,
    id: Option<u64>,
    definition: AutomationDefinition,
) -> Result<impl View> {
    let saved_enabled = definition.enabled;
    let editing = id.is_some();
    let opened = signal(cx, || false);
    let dialog_id = element_id(cx);
    let title_id = element_id(cx);
    let name = signal(cx, || definition.name.clone());
    let pattern = signal(cx, || definition.pattern.clone());
    let script = signal(cx, || definition.script.clone());
    let enabled = signal(cx, || definition.enabled);
    let consume = signal(cx, || definition.consume);
    let pending = signal(cx, || false);
    let error = signal(cx, String::new);
    let test_text = signal(cx, String::new);
    let test_result = signal(cx, String::new);
    let testing = signal(cx, || false);
    let outline = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary = button_variants(ButtonVariant::Primary, ButtonSize::Md);
    Ok(view! {
        <div class="flex flex-col gap-3">
            <button
                type="button"
                class=(outline.clone())
                commandfor=(&dialog_id)
                command="show-modal"
                aria-haspopup="dialog"
            >
                (if id.is_some() { "Edit" } else { "New automation" })
            </button>
            <dialog
                id=(&dialog_id)
                aria-labelledby=(&title_id)
                closedby="any"
                :open=$(opened.get())
                @toggle=$(move |_event| {
                    opened.set(
                        raw!(
                            "cx.hydrate(document.getElementById(${dialog_id}.toString()).open)",
                            false,
                        ),
                    );
                })
                class="fixed inset-0 m-auto max-h-[90dvh] w-[calc(100%-2rem)] max-w-3xl overflow-y-auto rounded-lg border border-border bg-background p-6 text-foreground shadow-lg backdrop:bg-black/60"
            >
                <div class="flex flex-col gap-4">
                    <h2 id=(&title_id) class="text-lg font-semibold">
                        (if editing { "Edit automation" } else { "New automation" })
                    </h2>
                    <label class="flex flex-col gap-1 text-sm">
                        "Name"
                        <input
                            aria-label="Automation name"
                            class="rounded-md border border-border bg-background p-2"
                            maxlength="80"
                            autofocus=(true)
                            value=(definition.name.clone())
                            :value=$(name.get())
                            @input=$(move |event: Event| name.set(event.target.value))
                        />
                    </label>
                    <label class="flex flex-col gap-1 text-sm">
                        "Pattern"
                        <input
                            aria-label="Message pattern"
                            class="rounded-md border border-border bg-background p-2 font-mono"
                            maxlength="1024"
                            placeholder="(?i)^example$"
                            value=(definition.pattern.clone())
                            :value=$(pattern.get())
                            @input=$(move |event: Event| pattern.set(event.target.value))
                        />
                    </label>
                    <label class="flex flex-col gap-1 text-sm">
                        "Script (Rhai)"
                        <textarea
                            aria-label="Rhai script"
                            class="min-h-64 w-full rounded-md border border-border bg-background p-3 font-mono text-sm"
                            rows="12"
                            maxlength="16384"
                            spellcheck="false"
                            :value=$(script.get())
                            @input=$(move |event: Event| script.set(event.target.value))
                        >
                            (definition.script.clone())
                        </textarea>
                    </label>
                    <div
                        class="flex flex-col gap-3 rounded-md border border-border p-3"
                    >
                        <label class="flex flex-col gap-1 text-sm">
                            <input
                                class="rounded-md border border-border bg-background p-2"
                                placeholder="Type a sample chat message"
                                maxlength="5000"
                                :value=$(test_text.get())
                                @input=$(move |event: Event| test_text.set(
                                        event.target.value,
                                    ))
                            />
                        </label>
                        <p class="text-xs text-muted-foreground">
                            "Works offline. Test alerts appear on every connected overlay, including live scenes. Test state is not saved."
                        </p>
                        <button
                            type="button"
                            class=(outline.clone())
                            :disabled=$(if testing.get() {
                                true
                            } else {
                                test_text.get().is_empty()
                            })
                            @click=$(async move |_event| {
                                testing.set(true);
                                test_result.set("Running…".to_owned());
                                test_result.set(
                                    test_automation(
                                        pattern.get(),
                                        script.get(),
                                        test_text.get(),
                                    ).await,
                                );
                                testing.set(false);
                            })
                        >
                            "Send test message"
                        </button>
                        <pre
                            role="status"
                            class="max-h-64 overflow-auto whitespace-pre-wrap break-words text-sm"
                            :hidden=$(test_result.get().is_empty())
                        >
                            $(test_result.get())
                        </pre>
                    </div>
                    <div class="flex flex-wrap gap-5 text-sm">
                        if !editing {
                            <label class="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    aria-label="Automation enabled"
                                    checked=(definition.enabled)
                                    :checked=$(enabled.get())
                                    @change=$(move |event: Event| enabled.set(
                                            event.target.checked,
                                        ))
                                />
                                "Enabled"
                            </label>
                        }
                        <label class="flex items-center gap-2">
                            <input
                                type="checkbox"
                                aria-label="Consume matched messages"
                                checked=(definition.consume)
                                :checked=$(consume.get())
                                @change=$(move |event: Event| consume.set(
                                        event.target.checked,
                                    ))
                            />
                            "Hide messages after handling"
                        </label>
                    </div>
                    <div class="flex gap-2">
                        <button
                            type="button"
                            class=(primary)
                            :disabled=$(pending.get())
                            @click=$(async move |_event| {
                                pending.set(true);
                                let next_enabled = if editing {
                                    saved_enabled
                                } else {
                                    enabled.get()
                                };
                                error.set(
                                    save_automation(
                                        id,
                                        name.get(),
                                        pattern.get(),
                                        script.get(),
                                        next_enabled,
                                        consume.get(),
                                    ).await,
                                );
                                if error.get().is_empty() {
                                    raw!(
                                        "document.getElementById(${dialog_id}.toString())?.close()",
                                        (),
                                    );
                                }
                                pending.set(false);
                            })
                        >
                            "Save"
                        </button>
                        <button
                            type="button"
                            class=(outline)
                            commandfor=(&dialog_id)
                            command="close"
                        >
                            "Cancel"
                        </button>
                    </div>
                    <p
                        role="alert"
                        class="text-sm text-destructive"
                        :hidden=$(error.get().is_empty())
                    >
                        $(error.get())
                    </p>
                </div>
            </dialog>
        </div>
    })
}

#[component]
async fn automation_row(
    cx: &Cx,
    automation: rtmp_core::chat::automations::Automation,
    is_live: bool,
) -> Result<impl View> {
    let id = automation.id;
    let enabled = automation.definition.enabled;
    let status = if !enabled {
        "Disabled"
    } else if is_live {
        "Enabled · live"
    } else {
        "Enabled · no active stream"
    };
    let pending = signal(cx, || false);
    let error = signal(cx, String::new);
    let outline = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    Ok(view! {
        <article
            class="flex flex-col gap-3 rounded-md border border-border p-4"
            data-automation-id=(id.to_string())
        >
            <div class="flex flex-wrap items-center justify-between gap-3">
                <div class="flex min-w-0 items-center gap-2">
                    <span
                        role="img"
                        aria-label=(status)
                        title=(status)
                        class=(if !enabled {
                            "hud-dot bg-live text-live"
                        } else if is_live {
                            "hud-dot bg-signal text-signal"
                        } else {
                            "hud-dot bg-warn text-warn"
                        })
                    ></span>
                    <h3 class="min-w-0 break-words font-semibold">
                        (automation.definition.name.clone())
                    </h3>
                </div>
                <div class="flex flex-wrap gap-2">
                    automation_editor(id: Some(id), definition: automation.definition)
                    <button
                        type="button"
                        class=(outline.clone())
                        :disabled=$(pending.get())
                        @click=$(async move |_event| {
                            pending.set(true);
                            error.set(toggle_automation(id, !enabled).await);
                            pending.set(false);
                        })
                    >
                        (if enabled { "Disable" } else { "Enable" })
                    </button>
                    <div
                        @click=$(async move |_event: Event| {
                            pending.set(true);
                            let next_error = delete_automation(id).await;
                            if !next_error.is_empty() {
                                error.set(next_error);
                                pending.set(false);
                            }
                        })
                    >
                        confirm_delete(
                            attrs: attributes! { type="button" :disabled=$(pending.get()) }
                        )
                    </div>
                </div>
            </div>
            <p
                role="alert"
                class="text-sm text-destructive"
                :hidden=$(error.get().is_empty())
            >
                $(error.get())
            </p>
        </article>
    })
}

#[shard]
pub async fn chat_automations(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        let app: &AppHandle = context(cx);
        let mut changes = app.chat.subscribe_changes();
        let mut stream = app.stream.subscribe_status();
        loop {
            changes.borrow_and_update();
            stream.borrow_and_update();
            let token = emit! { automations_panel() }?;
            if !connected(cx) {
                break Ok(token);
            }
            tokio::select! {
                result = changes.changed() => if result.is_err() { break Ok(token); },
                result = stream.changed() => if result.is_err() { break Ok(token); },
            }
        }
    })
}

#[component]
async fn automations_panel(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let snapshot = app
        .chat
        .automations()
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    let is_live = app.stream.status().state == StreamState::Live;
    Ok(view! {
        <section id="chat-automations" class="mt-8 pb-4" aria-label="Chat automations">
            card(
                card_header(card_title("Automations"))
                card_content(
                    <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                        <div
                            class="flex min-w-0 flex-col gap-5"
                            data-automation-list=""
                        >
                            #[key(automation.id)]
                            for automation in snapshot.automations {
                                automation_row(automation: automation, is_live: is_live)
                            }
                            automation_editor(
                                id: None,
                                definition: AutomationDefinition {
                                    name: String::new(),
                                    pattern: String::new(),
                                    script: rtmp_core::chat::automations::runtime::EXAMPLE_SCRIPT.to_owned(

                                    ),
                                    enabled: false,
                                    consume: false,
                                }
                            )
                        </div>
                        <div
                            class="min-w-0 border-t border-border pt-4 md:border-t-0 md:border-l md:pt-0 md:pl-5"
                            data-automation-log=""
                        >
                            <h3 class="mb-3 font-semibold">"Execution log"</h3>
                            if snapshot.recent.is_empty() {
                                <p class="text-sm text-muted-foreground">
                                    "No executions yet"
                                </p>
                            }
                            <div class="flex max-h-80 flex-col gap-2 overflow-y-auto">
                                for run in snapshot.recent {
                                    <div class="rounded-md border border-border p-3 text-sm">
                                        <p class="font-medium">
                                            (format!(
                                                "{} · {} · {}",
                                                run.automation_name,
                                                run.author,
                                                run.source,
                                            ))
                                        </p>
                                        if !run.error.is_empty() {
                                            <p class="break-words text-destructive">(run.error)</p>
                                        } else {
                                            <p class="break-words whitespace-pre-wrap">
                                                (if run.output.is_empty() {
                                                    "Completed".to_owned()
                                                } else {
                                                    run.output
                                                })
                                            </p>
                                        }
                                    </div>
                                }
                            </div>
                        </div>
                    </div>
                )
            )
        </section>
    })
}
