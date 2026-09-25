(() => {
  // Browser media and hls.js APIs are outside Topcoat's runtime vocabulary.
  const video = document.getElementById("stream-preview-video");
  const statusRefresh = document.getElementById("stream-status-refresh");
  if (!video) return;

  let player = null;
  let previewAttached = false;
  let lastStatusSignature = null;
  let previewReady = false;
  let retryTimer = null;

  function renderBitrate(bps) {
    if (typeof bps !== "number") return;
    const bitrate = document.querySelector('[data-preview-bitrate]');
    if (!bitrate) return;
    if (bps >= 1_000_000) bitrate.textContent = `${(bps / 1_000_000).toFixed(2)} Mbps`;
    else if (bps >= 1_000) bitrate.textContent = `${Math.round(bps / 1_000)} Kbps`;
    else bitrate.textContent = `${bps} bps`;
  }

  function detachPreview() {
    if (retryTimer) {
      clearTimeout(retryTimer);
      retryTimer = null;
    }
    if (player) {
      player.destroy();
      player = null;
    }
    video.removeAttribute("src");
    video.load();
    previewAttached = false;
  }

  function retryPreview() {
    if (retryTimer || !previewReady) return;
    retryTimer = setTimeout(() => {
      retryTimer = null;
      if (previewReady) attachPreview();
    }, 1000);
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
      player.attachMedia(video);
      player.loadSource(source);
      player.on(window.Hls.Events.MANIFEST_PARSED, () => video.play().catch(() => {}));
      player.on(window.Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        if (data.type === window.Hls.ErrorTypes.NETWORK_ERROR) {
          player.startLoad();
        } else if (data.type === window.Hls.ErrorTypes.MEDIA_ERROR) {
          player.recoverMediaError();
        } else {
          detachPreview();
          retryPreview();
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

    const recDot = document.querySelector('[data-preview-rec-dot]');
    const recLabel = document.querySelector('[data-preview-rec-label]');
    const isLive = status.state === "live";
    if (recDot) recDot.className = 'hud-dot ' + (isLive ? 'bg-red-500 text-red-500 animate-rec' : previewReady ? 'hud-dot bg-emerald-400 text-emerald-400' : 'hud-dot bg-white/30 text-white/30');
    if (recLabel) recLabel.textContent = isLive ? 'REC' : previewReady ? 'READY' : 'STBY';

    previewReady = status.state === "preview_ready" || status.state === "live";
    const previewFailed = status.state === "preview_failed";
    if (!previewReady || previewFailed) {
      if (previewAttached) detachPreview();
    }

    if (previewReady) attachPreview();
  }

  video.addEventListener("error", () => {
    if (!previewReady) return;
    detachPreview();
    retryPreview();
  });

  async function refresh() {
    try {
      const response = await fetch("/api/stream/status", { cache: "no-store" });
      if (response.ok) render(await response.json());
    } catch (_error) {
      // A later poll will recover after transient connection failures.
    }
  }

  window.addEventListener("rtmp:stream-status", (event) => render(event.detail));
  window.addEventListener("rtmp:metrics-history", (event) => {
    renderBitrate(event.detail.at(-1)?.ingest_bps);
  });
  window.addEventListener("rtmp:metrics-sample", (event) => {
    renderBitrate(event.detail?.ingest_bps);
  });
  refresh();
  if (!window.EventSource) window.setInterval(refresh, 1000);
})();
