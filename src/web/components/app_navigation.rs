use topcoat::{
    Result,
    context::Cx,
    router::{HrefTarget, href},
    runtime::{connected, link_attrs, prefetch_mode, shard},
    view::{View, component, emit, live, view},
};

const ACTIVE_LINK: &str = "rounded-[3px] bg-foreground px-2.5 py-1.5 font-mono text-[11px] font-semibold uppercase tracking-[0.12em] text-background transition-colors";
const INACTIVE_LINK: &str = "rounded-[3px] px-2.5 py-1.5 font-mono text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground transition-colors hover:bg-white/5 hover:text-foreground";

const NAV_ITEMS: &[(&str, &str, &(dyn HrefTarget + Sync))] = &[
    ("Preview", "preview", &crate::web::preview_page),
    ("Metrics", "metrics", &crate::web::metrics_page_route),
    ("Chat", "chat", &crate::web::chat_page),
    ("Logs", "logs", &crate::web::logs_page),
    ("Settings", "settings", &crate::web::settings_page),
    ("Targets", "targets", &crate::web::targets_page),
    ("Export", "export", &crate::web::export_page),
];

#[component]
pub async fn app_navigation(cx: &Cx, active_page: &'static str) -> Result<impl View> {
    Ok(view! {
        <header
            class="sticky top-0 z-20 border-b border-border bg-background/85 backdrop-blur-md"
        >
            <div
                class="mx-auto flex w-full max-w-[1600px] items-center gap-3 px-4 py-2 sm:px-5"
            >
                <a
                    (link_attrs(cx, href!(crate::web::preview_page), prefetch_mode(cx)))
                    class="flex shrink-0 items-center gap-2 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                    <img
                        src=(crate::web::FAVICON)
                        alt="RTMP-Manager"
                        width="24"
                        height="24"
                        class="size-6 shrink-0 rounded-[3px] border border-border object-contain"
                    />
                    <span class="leading-none">
                        <span
                            class="block font-mono text-[12px] font-bold tracking-[0.14em] text-foreground"
                        >
                            "RTMP-MGR"
                        </span>
                        <span
                            class="mt-0.5 block font-mono text-[9px] tracking-[0.18em] text-muted-foreground uppercase"
                        >
                            (env!("CARGO_PKG_VERSION"))
                        </span>
                    </span>
                </a>
                <nav
                    aria-label="Primary navigation"
                    class="hud-tabs -mx-1 flex flex-1 gap-0.5 overflow-x-auto px-0.5 py-0.5"
                >
                    for (label, id, target) in NAV_ITEMS {
                        let is_active = active_page == *id;
                        <a
                            (link_attrs(cx, href!(*target), prefetch_mode(cx)))
                            class=(if is_active { ACTIVE_LINK } else { INACTIVE_LINK })
                            aria-current=(if is_active { "page" } else { "false" })
                        >
                            (*label)
                        </a>
                    }
                </nav>
                <div class="hidden shrink-0 md:block">connection_status()</div>
            </div>
        </header>
    })
}

#[shard]
async fn connection_status(cx: &Cx) -> Result<impl View> {
    Ok(live! {
        loop {
            let linked = connected(cx);
            let token = emit! {
                <rtmp-connection
                    data-heartbeat=(if linked {
                        Some(crate::util::now_unix_ms().to_string())
                    } else {
                        None
                    })
                    class="flex items-center gap-1.5 font-mono text-[10px] tracking-[0.18em] text-muted-foreground uppercase"
                    role="status"
                    aria-live="polite"
                >
                    <span
                        data-control-link-dot="true"
                        class=(if linked {
                            "hud-dot bg-signal text-signal"
                        } else {
                            "hud-dot bg-white/30 text-white/30"
                        })
                        aria-hidden="true"
                    ></span>
                    <span data-control-link-label="true">
                        (if linked { "LINKED" } else { "CONNECTING" })
                    </span>
                </rtmp-connection>
            }?;
            if !linked {
                break Ok(token);
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    })
}
