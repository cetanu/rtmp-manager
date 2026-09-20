(() => {
  const key = new URLSearchParams(window.location.search).get("key");
  if (!key || !window.EventSource) return;

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
  });

  window.addEventListener("pagehide", () => {
    events.close();
  }, { once: true });
})();
