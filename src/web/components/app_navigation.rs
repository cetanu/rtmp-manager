use topcoat::{
    Result,
    view::{component, view},
};

const ACTIVE_LINK: &str = "rounded-md bg-primary px-2 py-1 text-xs font-medium text-primary-foreground shadow-xs transition-colors";
const INACTIVE_LINK: &str = "rounded-md px-2 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground";

fn link_class(active_page: &str, page: &str) -> &'static str {
    if active_page == page {
        ACTIVE_LINK
    } else {
        INACTIVE_LINK
    }
}

#[component]
pub async fn app_navigation(active_page: &'static str) -> Result {
    view! {
        <header class="sticky top-0 z-20 border-b border-border bg-background/95 backdrop-blur">
            <div class="mx-auto flex max-w-7xl items-center gap-2 px-3 py-2 sm:px-4">
                <a href="/preview" data-app-link="preview" class="shrink-0 rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring">
                    <span class="text-sm font-semibold leading-tight">"RTMP"<span class="hidden sm:inline">" Manager"</span></span>
                </a>
                <nav aria-label="Primary navigation" class="-mx-1 flex flex-1 gap-1 overflow-x-auto px-1">
                    <a
                        href="/preview"
                        data-app-link="preview"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "preview"))
                        aria-current=(if active_page == "preview" { "page" } else { "false" })
                    >"Preview"</a>
                    <a
                        href="/metrics"
                        data-app-link="metrics"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "metrics"))
                        aria-current=(if active_page == "metrics" { "page" } else { "false" })
                    >"Metrics"</a>
                    <a
                        href="/chat"
                        data-app-link="chat"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "chat"))
                        aria-current=(if active_page == "chat" { "page" } else { "false" })
                    >"Chat"</a>
                    <a
                        href="/logs"
                        data-app-link="logs"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "logs"))
                        aria-current=(if active_page == "logs" { "page" } else { "false" })
                    >"Logs"</a>
                    <a
                        href="/settings"
                        data-app-link="settings"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "settings"))
                        aria-current=(if active_page == "settings" { "page" } else { "false" })
                    >"Settings"</a>
                    <a
                        href="/targets"
                        data-app-link="targets"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "targets"))
                        aria-current=(if active_page == "targets" { "page" } else { "false" })
                    >"Targets"</a>
                    <a
                        href="/export"
                        data-app-link="export"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "export"))
                        aria-current=(if active_page == "export" { "page" } else { "false" })
                    >"Export"</a>
                </nav>
            </div>
        </header>
    }
}
