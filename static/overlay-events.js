(() => {
  const pad = (value) => String(value).padStart(2, "0");
  const formatRemaining = (endsAt) => {
    const secs = Math.max(0, Math.ceil((endsAt - Date.now()) / 1000));
    return `${pad(Math.floor(secs / 60))}:${pad(secs % 60)}`;
  };
  const tickCountdowns = () => {
    document.querySelectorAll("[data-pomodoro-countdown]").forEach((el) => {
      const endsAt = Number(el.dataset.endsAt || 0);
      if (!Number.isFinite(endsAt) || endsAt <= 0) return;
      const text = formatRemaining(endsAt);
      if (el.textContent !== text) el.textContent = text;
    });
  };

  const key = new URLSearchParams(window.location.search).get("key");
  if (key && window.EventSource) {
    const eventsUrl = new URL("/api/overlay/events", window.location.origin);
    eventsUrl.searchParams.set("key", key);
    const events = new EventSource(eventsUrl);

    events.addEventListener("chat", (event) => {
      const current = document.getElementById("chat-overlay-messages");
      if (!current) return;

      const template = document.createElement("template");
      template.innerHTML = event.data;
      const replacement = template.content.firstElementChild;
      if (replacement?.id !== "chat-overlay-messages") return;
      current.replaceWith(replacement);
      tickCountdowns();
    });

    window.addEventListener("pagehide", () => {
      events.close();
    }, { once: true });
  }

  tickCountdowns();
  window.setInterval(tickCountdowns, 1000);
})();
