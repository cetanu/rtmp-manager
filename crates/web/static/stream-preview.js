customElements.define("rtmp-preview", class extends HTMLElement {
  connectedCallback() {
    // Topcoat can connect an empty element before morphing its children.
    queueMicrotask(() => {
      if (this.isConnected && !this.cleanup) this.mount();
    });
  }

  mount() {
    if (this.cleanup) return;
    const root = this;
    // Browser media and hls.js APIs are outside Topcoat's runtime vocabulary.
    const video = root.querySelector("video");
    if (!video) return;

    let player = null;
    let previewAttached = false;
    let previewReady = false;
    let retryTimer = null;

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
      previewReady = status.state === "preview_ready" || status.state === "live";
      const previewFailed = status.state === "preview_failed";
      if (!previewReady || previewFailed) {
        if (previewAttached) detachPreview();
      }

      if (previewReady) attachPreview();
    }

    const abort = new AbortController();
    video.addEventListener("error", () => {
      if (!previewReady) return;
      detachPreview();
      retryPreview();
    }, { signal: abort.signal });

    let lastSignature = null;
    const refresh = () => {
      const status = root.querySelector("[data-preview-state]");
      if (!status) return;
      const signature = status.dataset.previewState;
      if (signature === lastSignature) return;
      lastSignature = signature;
      render({ state: status.dataset.previewState });
    };
    const observer = new MutationObserver(refresh);
    observer.observe(root, { childList: true, attributes: true, subtree: true });
    this.cleanup = () => {
      observer.disconnect();
      abort.abort();
      detachPreview();
    };
    refresh();
  }

  disconnectedCallback() {
    this.cleanup?.();
    this.cleanup = null;
  }
});
