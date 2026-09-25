(() => {
  // Live ticking for pomodoro countdowns rendered as [data-pomodoro-countdown].
  // Server HTML carries the initial MM:SS text plus data-ends-at (unix ms);
  // this keeps every countdown current without waiting for the next SSE refresh.
  const pad = (value) => String(value).padStart(2, "0");
  const tickCountdowns = () => {
    document.querySelectorAll("[data-pomodoro-countdown]").forEach((el) => {
      const endsAt = Number(el.dataset.endsAt || 0);
      if (!Number.isFinite(endsAt) || endsAt <= 0) return;
      const secs = Math.max(0, Math.ceil((endsAt - Date.now()) / 1000));
      const text = `${pad(Math.floor(secs / 60))}:${pad(secs % 60)}`;
      if (el.textContent !== text) el.textContent = text;
    });
  };
  tickCountdowns();
  window.setInterval(tickCountdowns, 1000);
})();

(() => {
  // Topcoat serves the SSE stream; its browser runtime has no EventSource binding yet.
  const linkDot = document.querySelector("[data-control-link-dot]");
  const linkLabel = document.querySelector("[data-control-link-label]");
  const setLinkStatus = (label, color) => {
    if (linkDot) linkDot.className = `hud-dot ${color}`;
    if (linkLabel) linkLabel.textContent = label;
  };

  if (!window.EventSource) {
    setLinkStatus("BROKEN", "bg-white/30 text-white/30");
    return;
  }

  const query = window.location.search || "";
  const events = new EventSource(`/api/events${query}`);
  events.addEventListener("open", () => setLinkStatus("LINKED", "bg-signal text-signal"));
  events.addEventListener("error", () => setLinkStatus("RETRYING", "bg-warn text-warn"));
  events.addEventListener("stream_status", (event) => {
    try {
      window.dispatchEvent(new CustomEvent("rtmp:stream-status", { detail: JSON.parse(event.data) }));
    } catch (_error) {}
  });
  events.addEventListener("chat_changed", () => {
    document.getElementById("chat-refresh-button")?.click();
    window.dispatchEvent(new CustomEvent("rtmp:chat-changed"));
  });
  events.addEventListener("metrics_history", (event) => {
    try {
      window.dispatchEvent(new CustomEvent("rtmp:metrics-history", { detail: JSON.parse(event.data) }));
    } catch (_error) {}
  });
  events.addEventListener("metrics_sample", (event) => {
    try {
      window.dispatchEvent(new CustomEvent("rtmp:metrics-sample", { detail: JSON.parse(event.data) }));
    } catch (_error) {}
  });

  window.addEventListener("pagehide", () => {
    events.close();
  }, { once: true });
})();
