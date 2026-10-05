// Heartbeats arrive through Topcoat's shared WebSocket. Detect silence locally
// because a disconnected server cannot render the RETRYING state.
customElements.define("rtmp-connection", class extends HTMLElement {
  static observedAttributes = ["data-heartbeat"];

  connectedCallback() {
    queueMicrotask(() => {
      if (this.isConnected) this.refresh();
    });
  }

  attributeChangedCallback() {
    if (this.isConnected) this.refresh();
  }

  refresh() {
    clearTimeout(this.timer);
    if (!this.hasAttribute("data-heartbeat")) return;
    this.render("LINKED", "bg-signal text-signal");
    this.timer = setTimeout(() => {
      this.render("RETRYING", "bg-warn text-warn");
    }, 12000);
  }

  render(label, color) {
    const dot = this.querySelector("[data-control-link-dot]");
    const text = this.querySelector("[data-control-link-label]");
    if (dot) dot.className = `hud-dot ${color}`;
    if (text && text.textContent !== label) text.textContent = label;
  }

  disconnectedCallback() {
    clearTimeout(this.timer);
  }
});
