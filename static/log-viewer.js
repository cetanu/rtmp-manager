(() => {
  const output = document.querySelector("[data-log-output]");
  const status = document.querySelector("[data-log-status]");
  if (!output || !window.EventSource) return;

  const seen = new Set();
  let received = false;
  const source = new EventSource("/api/logs");

  function append(entry) {
    if (seen.has(entry.id)) return;
    seen.add(entry.id);
    if (!received) {
      output.replaceChildren();
      received = true;
    }
    const line = document.createElement("div");
    const timestamp = new Date(Number(entry.timestamp_ms)).toLocaleString();
    line.textContent = `${timestamp} ${entry.level.padEnd(5)} ${entry.target} — ${entry.message}`;
    line.className = entry.level === "ERROR" ? "text-red-400" : entry.level === "WARN" ? "text-amber-300" : "text-zinc-200";
    const followsTail = output.scrollHeight - output.scrollTop - output.clientHeight < 40;
    output.append(line);
    if (followsTail) output.scrollTop = output.scrollHeight;
  }

  source.addEventListener("log", (event) => append(JSON.parse(event.data)));
  source.onopen = () => { status.textContent = "🟢 Connected - following live output"; };
  source.onerror = () => { status.textContent = "🔴 Disconnected - reconnecting..."; };
  document.querySelector("[data-log-clear]")?.addEventListener("click", () => {
    output.replaceChildren();
    seen.clear();
    received = true;
  });
})();
