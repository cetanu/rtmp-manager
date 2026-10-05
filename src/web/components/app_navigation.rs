use topcoat::{
    Result,
    context::Cx,
    router::href,
    runtime::{link_attrs, prefetch_mode},
    view::{View, component, view},
};

const ACTIVE_LINK: &str = "rounded-[3px] bg-foreground px-2.5 py-1.5 font-mono text-[11px] font-semibold uppercase tracking-[0.12em] text-background transition-colors";
const INACTIVE_LINK: &str = "rounded-[3px] px-2.5 py-1.5 font-mono text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground transition-colors hover:bg-white/5 hover:text-foreground";

fn link_class(active_page: &str, page: &str) -> &'static str {
    if active_page == page {
        ACTIVE_LINK
    } else {
        INACTIVE_LINK
    }
}

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
                    <a
                        (link_attrs(
                            cx,
                            href!(crate::web::preview_page),
                            prefetch_mode(cx),
                        ))
                        class=(link_class(active_page, "preview"))
                        aria-current=(if active_page == "preview" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Preview"
                    </a>
                    <a
                        (link_attrs(
                            cx,
                            href!(crate::web::metrics_page_route),
                            prefetch_mode(cx),
                        ))
                        class=(link_class(active_page, "metrics"))
                        aria-current=(if active_page == "metrics" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Metrics"
                    </a>
                    <a
                        (link_attrs(cx, href!(crate::web::chat_page), prefetch_mode(cx)))
                        class=(link_class(active_page, "chat"))
                        aria-current=(if active_page == "chat" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Chat"
                    </a>
                    <a
                        (link_attrs(cx, href!(crate::web::logs_page), prefetch_mode(cx)))
                        class=(link_class(active_page, "logs"))
                        aria-current=(if active_page == "logs" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Logs"
                    </a>
                    <a
                        (link_attrs(
                            cx,
                            href!(crate::web::settings_page),
                            prefetch_mode(cx),
                        ))
                        class=(link_class(active_page, "settings"))
                        aria-current=(if active_page == "settings" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Settings"
                    </a>
                    <a
                        (link_attrs(
                            cx,
                            href!(crate::web::targets_page),
                            prefetch_mode(cx),
                        ))
                        class=(link_class(active_page, "targets"))
                        aria-current=(if active_page == "targets" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Targets"
                    </a>
                    <a
                        (link_attrs(
                            cx,
                            href!(crate::web::export_page),
                            prefetch_mode(cx),
                        ))
                        class=(link_class(active_page, "export"))
                        aria-current=(if active_page == "export" {
                            "page"
                        } else {
                            "false"
                        })
                    >
                        "Export"
                    </a>
                </nav>
            </div>
        </header>
    })
}
