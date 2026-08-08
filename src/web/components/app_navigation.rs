use topcoat::{
    Result,
    view::{component, view},
};

const ACTIVE_LINK: &str = "rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground shadow-xs transition-colors";
const INACTIVE_LINK: &str = "rounded-lg px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground";

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
            <div class="mx-auto flex max-w-7xl flex-col gap-3 px-4 py-4 sm:flex-row sm:items-center sm:justify-between">
                <a href="/overview" data-app-link="overview" class="flex items-center gap-3 rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring">
                    <span class="flex size-9 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground shadow-xs">
                        "RM"
                    </span>
                    <span>
                        <span class="block text-sm font-semibold leading-tight">"RTMP Manager"</span>
                        <span class="block text-xs text-muted-foreground">"Stream control centre"</span>
                    </span>
                </a>
                <nav aria-label="Primary navigation" class="-mx-1 flex gap-1 overflow-x-auto px-1 pb-1 sm:mx-0 sm:pb-0">
                    <a
                        href="/overview"
                        data-app-link="overview"
                        data-active-class=(ACTIVE_LINK)
                        data-inactive-class=(INACTIVE_LINK)
                        class=(link_class(active_page, "overview"))
                        aria-current=(if active_page == "overview" { "page" } else { "false" })
                    >"Overview"</a>
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

#[component]
pub async fn page_heading(title: &'static str, description: &'static str) -> Result {
    view! {
        <div class="mb-6">
            <h1 data-page-heading="true" tabindex="-1" class="text-2xl font-semibold tracking-tight outline-none sm:text-3xl">
                (title)
            </h1>
            <p class="mt-2 max-w-3xl text-sm leading-relaxed text-muted-foreground sm:text-base">
                (description)
            </p>
        </div>
    }
}
