(() => {
  // Browser media and hls.js APIs are outside Topcoat's runtime vocabulary.
  const video = document.getElementById("stream-preview-video");
  const errorMessage = document.getElementById("stream-preview-error");
  const statusRefresh = document.getElementById("stream-status-refresh");
  if (!video || !errorMessage) return;

  let player = null;
  let previewAttached = false;
  let lastActive = false;
  let lastStatusSignature = null;
  let sessionId = null;

  function detachPreview() {
    if (player) {
      player.destroy();
      player = null;
    }
    video.removeAttribute("src");
    video.load();
    previewAttached = false;
  }

  function attachPreview() {
    if (previewAttached) return;
    const source = `/api/preview/index.m3u8?started=${Date.now()}`;
    if (video.canPlayType("application/vnd.apple.mpegurl")) {
      video.src = source;
      video.play().catch(() => {});
      previewAttached = true;
    } else if (window.Hls && window.Hls.isSupported()) {
      player = new window.Hls({
        liveSyncDurationCount: 2,
        liveMaxLatencyDurationCount: 5,
      });
      player.loadSource(source);
      player.attachMedia(video);
      player.on(window.Hls.Events.MANIFEST_PARSED, () => video.play().catch(() => {}));
      player.on(window.Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        if (data.type === window.Hls.ErrorTypes.NETWORK_ERROR) {
          player.startLoad();
        } else if (data.type === window.Hls.ErrorTypes.MEDIA_ERROR) {
          player.recoverMediaError();
        } else {
          detachPreview();
        }
      });
      previewAttached = true;
    } else {
      errorMessage.textContent = "This browser does not support HLS preview playback.";
      errorMessage.hidden = false;
    }
  }

  function render(status) {
    const statusSignature = JSON.stringify(status);
    if (statusSignature !== lastStatusSignature) {
      lastStatusSignature = statusSignature;
      statusRefresh?.click();
    }

    if (status.session_id !== sessionId) {
      detachPreview();
      sessionId = status.session_id;
      errorMessage.hidden = true;
      errorMessage.textContent = "";
    }
    if (!status.active) {
      if (lastActive) detachPreview();
    } else if (status.preview_failed) {
      if (previewAttached) detachPreview();
    }

    if (status.preview_ready) attachPreview();
    lastActive = status.active;
  }

  async function refresh() {
    try {
      const response = await fetch("/api/stream/status", { cache: "no-store" });
      if (response.ok) render(await response.json());
    } catch (_error) {
      // A later poll will recover after transient connection failures.
    }
  }

  window.addEventListener("rtmp:stream-status", (event) => render(event.detail));
  refresh();
  if (!window.EventSource) window.setInterval(refresh, 1000);
})();
