(() => {
  const refreshButton = document.getElementById("chat-refresh-button");
  if (!refreshButton || !window.EventSource) return;

  const events = new EventSource("/api/chat/events");
  events.addEventListener("chat", () => refreshButton.click());
  window.addEventListener("pagehide", () => events.close(), { once: true });
})();
