(() => {
  const STORAGE_KEY = "rtmp.chatMessageSizePx";
  const DEFAULT_PX = 14;
  const MIN_PX = 10;
  const MAX_PX = 32;
  const STEP_PX = 2;

  function clamp(value) {
    return Math.min(MAX_PX, Math.max(MIN_PX, value));
  }

  function loadSize() {
    try {
      const raw = Number(localStorage.getItem(STORAGE_KEY));
      if (Number.isFinite(raw) && raw >= MIN_PX && raw <= MAX_PX) {
        return raw;
      }
    } catch {}
    return DEFAULT_PX;
  }

  function saveSize(size) {
    try {
      localStorage.setItem(STORAGE_KEY, String(size));
    } catch {}
  }

  function syncButtons(size) {
    const dec = document.getElementById("chat-text-size-decrease");
    const inc = document.getElementById("chat-text-size-increase");
    if (dec) dec.disabled = size <= MIN_PX;
    if (inc) inc.disabled = size >= MAX_PX;
  }

  function applySize(size) {
    document.documentElement.style.setProperty("--chat-message-size", `${size}px`);
    syncButtons(size);
  }

  let currentSize = loadSize();
  applySize(currentSize);

  document.addEventListener("DOMContentLoaded", () => {
    syncButtons(currentSize);
  });

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-chat-size-step]");
    if (!button) return;
    const step = Number(button.getAttribute("data-chat-size-step"));
    if (!Number.isFinite(step) || step === 0) return;
    const next = clamp(currentSize + step * STEP_PX);
    if (next === currentSize) {
      syncButtons(currentSize);
      return;
    }
    currentSize = next;
    saveSize(currentSize);
    applySize(currentSize);
  });
})();
