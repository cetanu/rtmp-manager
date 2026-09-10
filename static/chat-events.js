(() => {
  // Topcoat serves the SSE stream; its browser runtime has no EventSource binding yet.
  if (!window.EventSource) return;

  const query = window.location.search || "";
  const events = new EventSource(`/api/events${query}`);
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
