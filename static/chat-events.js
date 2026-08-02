(() => {
  const refreshButton = document.getElementById("chat-refresh-button");
  if (!window.WebSocket) return;

  let reconnectDelay = 500;
  let reconnectTimer = null;
  let socket = null;
  let stopped = false;

  function connect() {
    if (stopped) return;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    socket = new WebSocket(`${protocol}//${window.location.host}/api/events`);

    socket.addEventListener("open", () => {
      reconnectDelay = 500;
    });
    socket.addEventListener("message", (event) => {
      let message;
      try {
        message = JSON.parse(event.data);
      } catch (_error) {
        return;
      }
      if (message.type === "stream_status") {
        window.dispatchEvent(new CustomEvent("rtmp:stream-status", { detail: message.data }));
      } else if (message.type === "chat_changed" && refreshButton) {
        refreshButton.click();
      }
    });
    socket.addEventListener("close", () => {
      if (stopped) return;
      reconnectTimer = window.setTimeout(connect, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, 10000);
    });
  }

  window.addEventListener("pagehide", () => {
    stopped = true;
    if (reconnectTimer !== null) window.clearTimeout(reconnectTimer);
    if (socket) socket.close();
  }, { once: true });
  connect();
})();
