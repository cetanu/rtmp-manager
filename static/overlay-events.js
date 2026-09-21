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

(() => {
  // Display options from the overlay URL: ?theme=plain|solid&size=sm|lg|xl
  // &align=top|bottom&direction=down|up|reverse&highlight=true|false plus the
  // numeric ?limit=, ?fade= and ?opacity=0..100 params. Kept in this external
  // file (rather than inline) so `<`, `>` and `&` survive HTML serving intact.
  const params = new URLSearchParams(window.location.search);
  const setChoice = (name, allowed) => {
    const value = params.get(name);
    if (allowed.includes(value)) document.body.dataset[name] = value;
  };
  setChoice("theme", ["plain", "solid"]);
  setChoice("size", ["sm", "lg", "xl"]);
  setChoice("align", ["top", "bottom"]);
  setChoice("direction", ["down", "up", "reverse"]);
  setChoice("highlight", ["true", "false"]);

  const limit = Number.parseInt(params.get("limit"), 10);
  if (Number.isInteger(limit) && limit >= 1 && limit <= 100) {
    const style = document.createElement("style");
    style.textContent = `.chat-overlay-message:nth-child(n+${limit + 1}) { display: none !important; }`;
    document.head.appendChild(style);
  }

  const fade = Number.parseInt(params.get("fade"), 10);
  if (Number.isInteger(fade) && fade > 0 && fade <= 3600) {
    const style = document.createElement("style");
    style.textContent = "@keyframes chatOverlayFade { 0%, 75% { opacity: 1; transform: translateY(0); } 100% { opacity: 0; transform: translateY(-4px); pointer-events: none; } } .chat-overlay-message { animation: chatOverlayFade " + fade + "s forwards ease-in-out; }";
    document.head.appendChild(style);
  }

  const opacity = Number.parseInt(params.get("opacity"), 10);
  if (Number.isInteger(opacity) && opacity >= 0 && opacity <= 100) {
    document.body.style.setProperty("--overlay-alpha", String(opacity / 100));
  }
})();
