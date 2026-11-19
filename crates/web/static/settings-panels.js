class SettingsPanels extends HTMLElement {
  #handlingInvalid = false;

  constructor() {
    super();
    this.addEventListener("click", (event) => {
      const tab = event.target.closest('[role="tab"]');
      if (tab) this.#select(tab);
    });
    this.addEventListener("keydown", (event) => {
      const tab = event.target.closest('[role="tab"]');
      if (!tab) return;

      const tabs = [...this.querySelectorAll('[role="tab"]')];
      const index = tabs.indexOf(tab);
      let next;
      switch (event.key) {
        case "ArrowLeft": next = (index + tabs.length - 1) % tabs.length; break;
        case "ArrowRight": next = (index + 1) % tabs.length; break;
        case "Home": next = 0; break;
        case "End": next = tabs.length - 1; break;
        default: return;
      }

      event.preventDefault();
      this.#select(tabs[next]);
      tabs[next].focus();
    });
    this.addEventListener("invalid", (event) => {
      if (this.#handlingInvalid) return;
      const panel = event.target.closest('[role="tabpanel"]');
      if (!panel) return;

      // Reveal the first invalid field before the browser tries to focus it.
      this.#handlingInvalid = true;
      this.#select(document.getElementById(panel.getAttribute("aria-labelledby")));
      queueMicrotask(() => { this.#handlingInvalid = false; });
    }, true);
  }

  #select(selected) {
    for (const tab of this.querySelectorAll('[role="tab"]')) {
      const active = tab === selected;
      tab.setAttribute("aria-selected", String(active));
      tab.tabIndex = active ? 0 : -1;
      document.getElementById(tab.getAttribute("aria-controls")).hidden = !active;
    }
  }
}

customElements.define("rtmp-settings-panels", SettingsPanels);
