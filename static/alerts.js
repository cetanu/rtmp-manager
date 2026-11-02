(() => {
  if (window.rtmpAlertsLoaded) return;
  window.rtmpAlertsLoaded = true;
  const key = new URLSearchParams(location.search).get("key");
  const mediaUrl = (name) => {
    const url = new URL("/api/alerts/media", location.origin);
    url.searchParams.set("name", name);
    if (key) url.searchParams.set("key", key);
    return url.href;
  };

  // One player per surface keeps image, text and audio on the same timeline.
  const player = (stage) => {
    const queue = [];
    let playing = false;
    let stopped = false;
    let timer;
    let audio;
    let generation = 0;
    const clear = () => {
      generation += 1;
      clearTimeout(timer);
      if (audio) { audio.pause(); audio.removeAttribute("src"); audio.load(); audio = null; }
      stage.replaceChildren();
    };
    const next = () => {
      clear();
      const alert = queue.shift();
      if (!alert || stopped) { playing = false; return; }
      playing = true;
      if (alert.image) {
        const img = document.createElement("img");
        img.src = mediaUrl(alert.image);
        img.alt = "";
        img.style.cssText = "max-width:90%;max-height:65%;object-fit:contain;flex-shrink:0";
        stage.append(img);
      }
      if (alert.text) {
        const text = document.createElement("div");
        text.textContent = alert.text;
        text.style.cssText = "box-sizing:border-box;width:80%;max-height:25%;overflow:hidden;white-space:pre-wrap;overflow-wrap:anywhere;text-align:center;flex-shrink:0";
        stage.append(text);
      }
      if (alert.sound) {
        audio = new Audio(mediaUrl(alert.sound));
        const currentGeneration = generation;
        audio.play().catch(() => {
          if (stopped || generation !== currentGeneration) return;
          const status = document.createElement("small");
          status.textContent = "Audio unavailable. Check the media file and browser audio permissions.";
          stage.append(status);
        });
      }
      timer = setTimeout(next, alert.duration_ms);
    };
    return {
      push(alert) { if (!stopped && queue.length < 64) { queue.push(alert); if (!playing) next(); } },
      stop() { stopped = true; queue.length = 0; clear(); },
    };
  };

  if (document.body.hasAttribute("data-alert-overlay")) {
    const playback = player(document.getElementById("alert-stage"));
    const url = new URL("/api/alerts/events", location.origin);
    if (key) url.searchParams.set("key", key);
    const events = new EventSource(url);
    events.addEventListener("alert", (event) => {
      try { playback.push(JSON.parse(event.data)); } catch (error) { console.error(error); }
    });
    window.addEventListener("pagehide", () => { events.close(); playback.stop(); }, { once: true });
  }

  document.addEventListener("change", (event) => {
    if (event.target.matches("[data-media-form] input[type=file]") && event.target.files.length) {
      event.target.form.requestSubmit();
    }
  });

  document.addEventListener("submit", async (event) => {
    const form = event.target.closest("[data-media-form]");
    if (!form) return;
    event.preventDefault();
    const library = form.closest("[data-media-library]");
    if (library.hasAttribute("aria-busy")) return;
    const status = library.querySelector("[data-media-status]");
    const body = new FormData(form);
    library.setAttribute("aria-busy", "true");
    status.classList.remove("hidden", "text-destructive");
    status.textContent = form.querySelector("input[type=file]") ? "Uploading media…" : "Deleting media…";
    library.querySelectorAll("button, input").forEach(control => { control.disabled = true; });
    try {
      const response = await fetch(form.action, { method: "POST", body });
      if (!response.ok) throw new Error(await response.text() || "Could not update media.");
      const template = document.createElement("template");
      template.innerHTML = await response.text();
      const replacement = template.content.firstElementChild;
      if (!replacement?.matches("[data-media-library]")) throw new Error("Could not refresh the media library.");
      library.replaceWith(replacement);
    } catch (error) {
      status.textContent = error.message;
      status.classList.add("text-destructive");
    } finally {
      library.removeAttribute("aria-busy");
      library.querySelectorAll("button, input").forEach(control => { control.disabled = false; });
      form.reset();
    }
  });

  document.addEventListener("click", async (event) => {
    const copy = event.target.closest("[data-copy-overlay], [data-copy-script]");
    if (copy) {
      const label = copy.textContent;
      try {
        const text = copy.dataset.copyScript ?? new URL(copy.dataset.copyOverlay, location.origin).href;
        await navigator.clipboard.writeText(text);
        copy.textContent = "Copied";
        setTimeout(() => { copy.textContent = label; }, 1500);
      } catch {
        const status = copy.closest("[data-media-library]").querySelector("[data-media-status]");
        status.classList.remove("hidden");
        status.textContent = "Clipboard unavailable in this browser. Open the OBS overlay to copy its URL, or use the script example below.";
      }
      return;
    }
  });
})();
