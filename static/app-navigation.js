(() => {
  // Topcoat 0.5 has no client-side navigation or History API binding.
  const pages = new Set(["overview", "chat", "settings", "targets", "export"]);
  const pageTitles = {
    overview: "Overview",
    chat: "Chat",
    settings: "Settings",
    targets: "Targets",
    export: "Export",
  };
  const pageSections = Array.from(document.querySelectorAll("[data-app-page]"));
  const configForm = document.querySelector("[data-app-config]");
  const returnTo = document.querySelector("[data-app-return-to]");
  const links = Array.from(document.querySelectorAll("[data-app-link]"));

  function pageFromPath() {
    const page = window.location.pathname.replace(/^\/+|\/+$/g, "");
    return pages.has(page) ? page : "overview";
  }

  function showPage(page, updateHistory, focusHeading) {
    if (!pages.has(page)) page = "overview";

    pageSections.forEach((section) => {
      section.hidden = section.dataset.appPage !== page;
    });

    if (configForm) configForm.hidden = page !== "settings" && page !== "targets";
    if (returnTo) returnTo.value = page === "targets" ? "/targets" : "/settings";

    links.forEach((link) => {
      const active = link.dataset.appLink === page;
      if (link.dataset.activeClass && link.dataset.inactiveClass) {
        link.className = active ? link.dataset.activeClass : link.dataset.inactiveClass;
      }
      if (active) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    });

    document.title = `${pageTitles[page]} · RTMP Manager`;
    if (updateHistory && window.location.pathname !== `/${page}`) {
      window.history.pushState({ page }, "", `/${page}`);
    }
    if (focusHeading) {
      document.querySelector(`[data-app-page="${page}"] [data-page-heading]`)?.focus();
    }
  }

  links.forEach((link) => {
    link.addEventListener("click", (event) => {
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      showPage(link.dataset.appLink, true, true);
    });
  });

  window.addEventListener("popstate", () => showPage(pageFromPath(), false, false));
  const initialPage = pageFromPath();
  if (window.location.pathname === "/") window.history.replaceState({ page: initialPage }, "", `/${initialPage}`);
  showPage(initialPage, false, false);
})();
