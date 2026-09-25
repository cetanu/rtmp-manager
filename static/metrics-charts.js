(() => {
  const cards = Array.from(document.querySelectorAll("[data-target-metric]"));
  const ingestCard = document.querySelector("[data-ingest-metric]");
  const transferCard = document.querySelector("[data-transfer-metric]");
  const chatMessagesCard = document.querySelector("[data-chat-messages-metric]");
  if (!cards.length && !ingestCard && !transferCard && !chatMessagesCard) return;
  let samples = [];

  const formatRate = (bps) => {
    if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
    if (bps >= 1_000) return `${Math.round(bps / 1_000)} Kbps`;
    return `${bps || 0} bps`;
  };

  const formatBytes = (bytes) => {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(2)} MB`;
    if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
    return `${bytes || 0} B`;
  };

  function draw(canvas, values, color, label, formatValue, minimum) {
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(180, Math.floor(rect.width));
    const height = Math.max(1, Math.floor(rect.height));
    canvas.width = width * ratio;
    canvas.height = height * ratio;
    const context = canvas.getContext("2d");
    context.scale(ratio, ratio);
    context.clearRect(0, 0, width, height);

    const maximum = Math.max(minimum, ...values);
    const left = 44;
    const top = 12;
    const plotWidth = width - left - 8;
    const plotHeight = height - top - 28;

    context.strokeStyle = "rgba(148, 163, 184, .14)";
    context.fillStyle = "rgb(148, 163, 184)";
    context.font = '10px "JetBrains Mono", monospace';
    for (let index = 0; index <= 3; index += 1) {
      const y = top + (plotHeight * index) / 3;
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(width - 8, y);
      context.stroke();
      context.fillText(formatValue(maximum * (1 - index / 3)), 0, y + 4);
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

  function drawMulti(canvas, series, formatValue, minimum) {
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(180, Math.floor(rect.width));
    const height = Math.max(1, Math.floor(rect.height));
    canvas.width = width * ratio;
    canvas.height = height * ratio;
    const context = canvas.getContext("2d");
    context.scale(ratio, ratio);
    context.clearRect(0, 0, width, height);

    const allValues = series.flatMap((entry) => entry.values);
    const maximum = Math.max(minimum, ...allValues);
    const left = 44;
    const top = 12;
    const plotWidth = width - left - 8;
    const plotHeight = height - top - 28;

    context.strokeStyle = "rgba(148, 163, 184, .14)";
    context.fillStyle = "rgb(148, 163, 184)";
    context.font = '10px "JetBrains Mono", monospace';
    for (let index = 0; index <= 3; index += 1) {
      const y = top + (plotHeight * index) / 3;
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(width - 8, y);
      context.stroke();
      context.fillText(formatValue(maximum * (1 - index / 3)), 0, y + 4);
    }

    series.forEach((entry) => {
      context.strokeStyle = entry.color;
      context.lineWidth = 2;
      context.beginPath();
      entry.values.forEach((value, index) => {
        const x = left + (plotWidth * index) / Math.max(1, entry.values.length - 1);
        const y = top + plotHeight * (1 - value / maximum);
        if (index === 0) context.moveTo(x, y);
        else context.lineTo(x, y);
      });
      context.stroke();
    });

    let legendX = left;
    context.font = '10px "JetBrains Mono", monospace';
    series.forEach((entry) => {
      context.fillStyle = entry.color;
      context.fillRect(legendX, height - 10, 10, 3);
      legendX += 15;
      context.fillStyle = "rgb(148, 163, 184)";
      context.fillText(entry.label, legendX, height - 6);
      legendX += context.measureText(entry.label).width + 12;
    });
  }

  function render() {
    if (ingestCard) {
      const latest = samples.at(-1)?.ingest_bps || 0;
      ingestCard.querySelector("[data-ingest-bitrate]").textContent = formatRate(latest);
      draw(
        ingestCard.querySelector("canvas"),
        samples.map((sample) => sample.ingest_bps || 0),
        "rgb(74, 222, 128)",
        "Ingest",
        formatRate,
        1_000_000,
      );
    }
    if (transferCard) {
      const latestIn = samples.at(-1)?.ingest_bytes || 0;
      const latestOut = samples.at(-1)?.egress_bytes || 0;
      const inEl = transferCard.querySelector("[data-transfer-in]");
      const outEl = transferCard.querySelector("[data-transfer-out]");
      const totalEl = transferCard.querySelector("[data-transfer-total]");
      if (inEl) inEl.textContent = formatBytes(latestIn);
      if (outEl) outEl.textContent = formatBytes(latestOut);
      if (totalEl) totalEl.textContent = formatBytes(latestIn + latestOut);
      const inValues = samples.map((sample) => sample.ingest_bytes || 0);
      const outValues = samples.map((sample) => sample.egress_bytes || 0);
      const totalValues = samples.map(
        (sample) => (sample.ingest_bytes || 0) + (sample.egress_bytes || 0),
      );
      drawMulti(
        transferCard.querySelector("canvas"),
        [
          { values: inValues, color: "rgb(74, 222, 128)", label: "In" },
          { values: outValues, color: "rgb(56, 189, 248)", label: "Out" },
          { values: totalValues, color: "rgb(251, 191, 36)", label: "Total" },
        ],
        formatBytes,
        1_000,
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
        "rgb(56, 189, 248)",
        "Outbound",
        formatRate,
        1_000_000,
      );
    });
    if (chatMessagesCard) {
      const latest = samples.at(-1)?.chat_messages_received || 0;
      chatMessagesCard.querySelector("[data-chat-messages-received]").textContent =
        latest.toLocaleString();
      draw(
        chatMessagesCard.querySelector("canvas"),
        samples.map((sample) => sample.chat_messages_received || 0),
        "rgb(74, 222, 128)",
        "Messages",
        (value) => Math.round(value).toLocaleString(),
        1,
      );
    }
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
