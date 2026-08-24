(() => {
  // Browser media and hls.js APIs are outside Topcoat's runtime vocabulary.
  const video = document.getElementById("stream-preview-video");
  const statusRefresh = document.getElementById("stream-status-refresh");
  if (!video) return;

  let player = null;
  let previewAttached = false;
  let lastStatusSignature = null;

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
      console.error("This browser does not support HLS preview playback.");
    }
  }

  function render(status) {
    const statusSignature = JSON.stringify(status);
    if (statusSignature !== lastStatusSignature) {
      lastStatusSignature = statusSignature;
      statusRefresh?.click();
    }

    const previewReady = status.state === "preview_ready" || status.state === "live";
    const previewFailed = status.state === "preview_failed";
    if (!previewReady || previewFailed) {
      if (previewAttached) detachPreview();
    }

    if (previewReady) attachPreview();
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
