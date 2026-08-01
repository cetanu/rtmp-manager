(() => {
  const video = document.getElementById("stream-preview-video");
  const placeholder = document.getElementById("stream-preview-placeholder");
  const badge = document.getElementById("stream-stage-badge");
  const detail = document.getElementById("stream-stage-detail");
  const publishButton = document.getElementById("publish-staged-stream");
  const stopButton = document.getElementById("stop-publishing-stream");
  const errorMessage = document.getElementById("stream-action-error");
  if (!video || !placeholder || !badge || !detail || !publishButton || !stopButton) return;

  let player = null;
  let previewAttached = false;
  let lastActive = false;
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
      errorMessage.classList.remove("hidden");
    }
  }

  function render(status) {
    if (status.session_id !== sessionId) {
      detachPreview();
      sessionId = status.session_id;
    }
    if (!status.active) {
      if (lastActive) detachPreview();
      badge.textContent = "Offline";
      badge.dataset.state = "offline";
      detail.textContent = "Waiting for an RTMP stream. Nothing will be sent to external targets automatically.";
      placeholder.textContent = "Start streaming to the RTMP ingest to create a preview.";
      placeholder.classList.remove("hidden");
    } else if (status.preview_failed) {
      if (previewAttached) detachPreview();
      badge.textContent = "Preview error";
      badge.dataset.state = "error";
      detail.textContent = "The HLS preview process stopped. Check the server logs, then reconnect the RTMP stream.";
      placeholder.textContent = "HLS preview unavailable.";
      placeholder.classList.remove("hidden");
    } else if (status.published) {
      badge.textContent = "Live";
      badge.dataset.state = "live";
      detail.textContent = "Publishing to enabled targets. The local preview remains available.";
      placeholder.classList.toggle("hidden", status.preview_ready);
    } else {
      badge.textContent = "Staged";
      badge.dataset.state = "staged";
      detail.textContent = status.preview_ready
        ? "Preview ready. Review it before publishing to enabled targets."
        : "Stream connected. Preparing the HLS preview…";
      placeholder.textContent = "Preparing HLS preview…";
      placeholder.classList.toggle("hidden", status.preview_ready);
    }

    if (status.preview_ready) attachPreview();
    publishButton.disabled = !status.active || status.published;
    stopButton.disabled = !status.active || !status.published;
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

  async function performAction(url) {
    errorMessage.classList.add("hidden");
    publishButton.disabled = true;
    stopButton.disabled = true;
    try {
      const response = await fetch(url, { method: "POST" });
      if (!response.ok) {
        errorMessage.textContent = (await response.text()) || "The stream action failed.";
        errorMessage.classList.remove("hidden");
      }
    } catch (_error) {
      errorMessage.textContent = "The server could not be reached.";
      errorMessage.classList.remove("hidden");
    }
    await refresh();
  }

  publishButton.addEventListener("click", () => performAction("/api/stream/publish"));
  stopButton.addEventListener("click", () => performAction("/api/stream/stop-publishing"));
  refresh();
  window.setInterval(refresh, 1000);
})();
