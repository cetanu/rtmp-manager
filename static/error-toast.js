// Toast error notification component and global error handler.
// Listens for 'rtmp:error' events, window errors, unhandled promise rejections,
// and URL query parameters containing error messages (?error=...).

class ErrorToaster extends HTMLElement {
  static observedAttributes = ["style", "class"];

  connectedCallback() {
    this.ensureStyles();

    window.addEventListener("rtmp:error", (e) => {
      const msg = e.detail?.message || (typeof e.detail === "string" ? e.detail : null);
      if (msg) this.show(msg);
    });

    // Check query params for error on page load / navigation
    this.checkQueryError();

    // Observe DOM mutations to catch errors surfaced into error elements
    this.observeInlineErrors();
  }

  attributeChangedCallback() {
    this.ensureStyles();
  }

  ensureStyles() {
    if (this.applyingStyles) return;
    const desired =
      "position: fixed !important; top: 1rem !important; right: 1rem !important; z-index: 9999 !important; display: flex !important; flex-direction: column !important; gap: 0.5rem !important; pointer-events: none !important; max-width: 24rem !important; width: calc(100% - 2rem) !important; padding: 0 !important; margin: 0 !important; box-sizing: border-box !important;";
    if (this.style.cssText !== desired) {
      // Setting style synchronously invokes attributeChangedCallback again.
      this.applyingStyles = true;
      try { this.style.cssText = desired; }
      finally { this.applyingStyles = false; }
    }
  }

  observeInlineErrors() {
    const isErrorEl = (node) => {
      if (!node || node.nodeType !== Node.ELEMENT_NODE) return false;
      return node.getAttribute?.("role") === "alert" && !node.closest("error-toaster");
    };

    const checkElement = (el) => {
      if (!isErrorEl(el)) return;
      if (el.closest("[hidden]") || el.getClientRects().length === 0) return;
      const text = el.textContent?.trim();
      if (text) {
        this.show(text);
      }
    };

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === "childList") {
          for (const node of mutation.addedNodes) {
            checkElement(node);
            if (node.querySelectorAll) {
              node.querySelectorAll("[role='alert']").forEach(checkElement);
            }
          }
        } else if (mutation.type === "characterData") {
          checkElement(mutation.target.parentElement);
        } else if (mutation.type === "attributes") {
          checkElement(mutation.target);
        }
      }
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      attributeFilter: ["hidden", "style", "class"],
    });
  }

  checkQueryError() {
    try {
      const url = new URL(window.location.href);
      const err = url.searchParams.get("error");
      if (err) {
        this.show(err);
        url.searchParams.delete("error");
        window.history.replaceState({}, "", url.pathname + (url.search ? url.search : "") + url.hash);
      }
    } catch (_e) {}
  }

  show(message, durationMs = 5000) {
    if (!message || typeof message !== "string") return;
    const trimmed = message.trim();
    if (!trimmed || trimmed.toLowerCase() === "error") return;

    // Deduplicate identical toasts already showing
    for (const existing of this.querySelectorAll("[data-toast-message]")) {
      if (existing.textContent === trimmed) return;
    }

    const toast = document.createElement("div");
    toast.className =
      "pointer-events-auto flex items-start justify-between gap-3 p-3.5 rounded-lg border border-live/40 bg-panel/95 text-foreground shadow-lg backdrop-blur-md transition-all duration-300 ease-out translate-y-[-8px] opacity-0";
    toast.setAttribute("role", "alert");
    toast.setAttribute("aria-live", "assertive");

    const body = document.createElement("div");
    body.className = "flex items-start gap-2.5 min-w-0 flex-1";

    // Destructive/warning indicator dot
    const dot = document.createElement("span");
    dot.className = "hud-dot bg-live text-live shrink-0 mt-1";
    dot.setAttribute("aria-hidden", "true");

    const content = document.createElement("div");
    content.className = "min-w-0 flex-1";

    const title = document.createElement("p");
    title.className = "font-mono text-[11px] font-bold tracking-[0.12em] text-live uppercase";
    title.textContent = "Error";

    const text = document.createElement("p");
    text.className = "text-xs break-words text-foreground mt-0.5";
    text.setAttribute("data-toast-message", "");
    text.textContent = trimmed;

    content.appendChild(title);
    content.appendChild(text);
    body.appendChild(dot);
    body.appendChild(content);

    // Dismiss button
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className =
      "shrink-0 rounded p-1 text-muted-foreground hover:text-foreground hover:bg-white/10 transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring";
    closeBtn.setAttribute("aria-label", "Dismiss error");
    closeBtn.innerHTML =
      '<svg class="size-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>';

    toast.appendChild(body);
    toast.appendChild(closeBtn);

    let dismissTimer = null;

    const dismiss = () => {
      if (dismissTimer) clearTimeout(dismissTimer);
      toast.classList.remove("opacity-100", "translate-y-0");
      toast.classList.add("opacity-0", "translate-y-[-8px]");
      setTimeout(() => {
        if (toast.parentElement) toast.parentElement.removeChild(toast);
      }, 300);
    };

    closeBtn.addEventListener("click", dismiss);

    dismissTimer = setTimeout(dismiss, durationMs);

    this.appendChild(toast);

    // Trigger enter transition on next animation frame
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        toast.classList.remove("opacity-0", "translate-y-[-8px]");
        toast.classList.add("opacity-100", "translate-y-0");
      });
    });
  }
}

if (!customElements.get("error-toaster")) {
  customElements.define("error-toaster", ErrorToaster);
}

// Global helper to trigger error notifications
window.showError = function (message, durationMs) {
  let toaster = document.querySelector("error-toaster");
  if (!toaster) {
    toaster = document.createElement("error-toaster");
    document.body.appendChild(toaster);
  }
  toaster.show(message, durationMs);
};

// Intercept console.error to capture Topcoat runtime errors and other explicit log errors
const originalConsoleError = console.error;
console.error = function (...args) {
  originalConsoleError.apply(console, args);
  try {
    const first = args[0];
    let msg = "";
    if (typeof first === "string" && first.startsWith("[topcoat]")) {
      const err = args[1];
      if (err instanceof Error && err.message) {
        msg = err.message;
      } else if (typeof err === "string" && err.trim()) {
        msg = err;
      }
    } else if (first instanceof Error && first.message) {
      msg = first.message;
    }
    // Avoid showing empty or bare "Error" toasts
    if (msg && msg.trim() && msg.trim().toLowerCase() !== "error") {
      window.showError(msg);
    }
  } catch (_e) {}
};

// Listen for global unhandled errors
window.addEventListener("error", (event) => {
  // Ignore harmless resize observer loop notifications
  if (event.message && event.message.includes("ResizeObserver loop")) return;
  const msg = event.message || (event.error && event.error.message);
  if (msg && msg.trim() && msg.trim().toLowerCase() !== "error") {
    window.showError(msg);
  }
});

// Listen for unhandled promise rejections
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  const msg = typeof reason === "string" ? reason : reason?.message;
  if (msg && msg.trim() && msg.trim().toLowerCase() !== "error") {
    window.showError(msg);
  }
});
