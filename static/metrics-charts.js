(() => {
  const cards = Array.from(document.querySelectorAll("[data-target-metric]"));
  const ingestCard = document.querySelector("[data-ingest-metric]");
  if (!cards.length && !ingestCard) return;
  let samples = [];

  const formatRate = (bps) => {
    if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
    if (bps >= 1_000) return `${Math.round(bps / 1_000)} Kbps`;
    return `${bps || 0} bps`;
  };

  function draw(canvas, values, color, label) {
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(180, Math.floor(rect.width));
    const height = Math.max(1, Math.floor(rect.height));
    canvas.width = width * ratio;
    canvas.height = height * ratio;
    const context = canvas.getContext("2d");
    context.scale(ratio, ratio);
    context.clearRect(0, 0, width, height);

    const maximum = Math.max(1_000_000, ...values);
    const left = 44;
    const top = 12;
    const plotWidth = width - left - 8;
    const plotHeight = height - top - 28;

    context.strokeStyle = "rgba(148, 163, 184, .18)";
    context.fillStyle = "rgb(148, 163, 184)";
    context.font = "11px Inter, sans-serif";
    for (let index = 0; index <= 3; index += 1) {
      const y = top + (plotHeight * index) / 3;
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(width - 8, y);
      context.stroke();
      context.fillText(formatRate(maximum * (1 - index / 3)), 0, y + 4);
    }

    context.strokeStyle = color;
    context.lineWidth = 2;
    context.beginPath();
    values.forEach((value, index) => {
      const x = left + (plotWidth * index) / Math.max(1, values.length - 1);
      const y = top + plotHeight * (1 - value / maximum);
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    });
    context.stroke();

    context.fillStyle = color;
    context.fillRect(left, height - 10, 10, 3);
    context.fillStyle = "rgb(148, 163, 184)";
    context.fillText(label, left + 15, height - 6);
  }

  function render() {
    const status = document.querySelector("[data-metrics-status]");
    if (ingestCard) {
      const latest = samples.at(-1)?.ingest_bps || 0;
      ingestCard.querySelector("[data-ingest-bitrate]").textContent = formatRate(latest);
      draw(
        ingestCard.querySelector("canvas"),
        samples.map((sample) => sample.ingest_bps || 0),
        "rgb(56, 189, 248)",
        "Ingest",
      );
    }
    cards.forEach((card) => {
      const name = card.dataset.targetMetric;
      const latest = [...samples].reverse().map((sample) =>
        sample.targets.find((target) => target.name === name)).find(Boolean);
      card.querySelector("[data-bitrate-out]").textContent = formatRate(latest?.outbound_bps || 0);
      draw(
        card.querySelector("canvas"),
        samples.map((sample) =>
          sample.targets.find((target) => target.name === name)?.outbound_bps || 0),
        "rgb(167, 139, 250)",
        "Outbound",
      );
    });
    if (status) status.textContent = "Live · updated now";
  }

  window.addEventListener("rtmp:metrics-history", (event) => {
    samples = event.detail;
    render();
  });
  window.addEventListener("rtmp:metrics-sample", (event) => {
    if (!event.detail) return;
    if (samples.at(-1)?.timestamp_ms !== event.detail.timestamp_ms) samples.push(event.detail);
    if (samples.length > 300) samples.shift();
    render();
  });
  window.addEventListener("resize", render);
})();
