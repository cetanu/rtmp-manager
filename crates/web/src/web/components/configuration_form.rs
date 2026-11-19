use crate::web::components::actions_panel::actions_panel;
use crate::web::components::chat_settings::{
    chat_ingest_settings, focus_poll_settings, kick_settings, twitch_settings, x_settings,
    youtube_settings,
};
use crate::web::components::notifications::notifications;
use crate::web::components::server_settings::server_settings;
use crate::web::components::targets::targets;
use crate::web::components::web_auth::web_auth;
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Child, View, component, view},
};

const SETTINGS_SECTIONS: &[(&str, &str)] = &[
    ("ingest", "Stream ingest"),
    ("access", "Access & security"),
    ("platforms", "Platforms"),
    ("chat-tools", "Chat & overlays"),
    ("notifications", "Notifications"),
];

#[component]
pub async fn settings_section(cx: &Cx, title: &'static str, child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <section
            class="grid gap-4 p-4 sm:grid-cols-[11rem_minmax(0,1fr)] sm:gap-6 sm:p-6"
        >
            <h2 class="hud-label text-foreground">(title)</h2>
            <div class="grid min-w-0 items-start gap-4 md:grid-cols-2">(child)</div>
        </section>
    })
}

#[component]
pub async fn configuration_form(cx: &Cx, active_page: &'static str) -> Result<impl View> {
    let is_settings = active_page == "settings";
    Ok(view! {
        <form id="configForm" method="post" action="/api/config" class="relative">
            <input
                type="hidden"
                name="return_to"
                value=(if active_page == "targets" { "/targets" } else { "/settings" })
            />
            if is_settings {
                <rtmp-settings-panels class="block">
                    <div
                        role="tablist"
                        aria-label="Settings panels"
                        class="flex overflow-x-auto border-b border-border"
                    >
                        for (index, (id, label)) in SETTINGS_SECTIONS.iter().enumerate() {
                            <button
                                type="button"
                                role="tab"
                                id=(format!("settings-tab-{id}"))
                                aria-controls=(format!("settings-panel-{id}"))
                                aria-selected=(if index == 0 { "true" } else { "false" })
                                tabindex=(if index == 0 { "0" } else { "-1" })
                                class="hud-label shrink-0 border-b-2 border-transparent px-3 py-2.5 whitespace-nowrap hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring aria-selected:border-signal aria-selected:bg-panel aria-selected:text-foreground"
                            >
                                (*label)
                            </button>
                        }
                    </div>
                    #[key(*id)]
                    for (index, (id, _)) in SETTINGS_SECTIONS.iter().enumerate() {
                        <div
                            id=(format!("settings-panel-{id}"))
                            role="tabpanel"
                            aria-labelledby=(format!("settings-tab-{id}"))
                            hidden=(index != 0)
                            class="mt-3"
                        >
                            <div class="hud-panel gap-0 divide-y divide-border">
                                match *id {
                                    "ingest" => {
                                        server_settings()
                                    }
                                    "access" => {
                                        web_auth()
                                    }
                                    "platforms" => {
                                        twitch_settings()
                                        youtube_settings()
                                        kick_settings()
                                        x_settings()
                                    }
                                    "chat-tools" => {
                                        chat_ingest_settings()
                                        focus_poll_settings()
                                    }
                                    "notifications" => {
                                        notifications()
                                    }
                                    _ => {

                                    }
                                }
                            </div>
                        </div>
                    }
                </rtmp-settings-panels>
            } else {
                targets()
            }
            <div
                class=(if is_settings {
                    "mt-4 sm:sticky sm:bottom-0 sm:z-10 sm:bg-background sm:py-3"
                } else {
                    "mt-6"
                })
            >
                actions_panel()
            </div>
        </form>
    })
}
