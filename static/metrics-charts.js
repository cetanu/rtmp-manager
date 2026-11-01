customElements.define("rtmp-metrics", class extends HTMLElement {
  connectedCallback() {
    queueMicrotask(() => {
      if (this.isConnected && !this.cleanup) this.mount();
    });
    if (!this.cleanup && !this.observer) {
      this.observer = new MutationObserver(() => {
        if (this.isConnected && !this.cleanup) this.mount();
      });
      this.observer.observe(this, { childList: true, subtree: true });
    }
  }

  mount() {
    if (this.cleanup) return;
    const cards = Array.from(this.querySelectorAll("[data-target-metric]"));
    const ingestCard = this.querySelector("[data-ingest-metric]");
    const transferCards = Array.from(this.querySelectorAll("[data-transfer-metric]"));
    const chatOutcomeCards = Array.from(this.querySelectorAll("[data-chat-outcomes]"));
    const chatMessagesCard = this.querySelector("[data-chat-messages-metric]");
    if (!cards.length && !ingestCard && !transferCards.length && !chatMessagesCard && !chatOutcomeCards.length) return;

    this.observer?.disconnect();
    this.observer = null;

    const abort = new AbortController();
    this.cleanup = () => abort.abort();
    let samples = [];

    const formatRate = (bps) => {
      if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(2)} Mbps`;
      if (bps >= 1_000) return `${Math.round(bps / 1_000)} Kbps`;
      return `${Math.round(bps || 0)} bps`;
    };

    const formatBytes = (bytes) => {
      if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
      if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(2)} MB`;
      if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
      return `${Math.round(bytes || 0)} B`;
    };

    function draw(canvas, values, color, label, formatValue, minimum, additionalSeries = []) {
      const rect = canvas.getBoundingClientRect();
      const ratio = window.devicePixelRatio || 1;
      const width = Math.max(180, Math.floor(rect.width));
      const height = Math.max(1, Math.floor(rect.height));
      canvas.width = width * ratio;
      canvas.height = height * ratio;
      const context = canvas.getContext("2d");
      context.scale(ratio, ratio);
      context.clearRect(0, 0, width, height);

      const maximum = Math.max(minimum, ...values, ...additionalSeries.flatMap((series) => series.values));
      context.font = '10px "JetBrains Mono", monospace';
      const tickLabels = Array.from({ length: 4 }, (_, index) =>
        formatValue(maximum * (1 - index / 3)));
      const left = Math.ceil(Math.max(...tickLabels.map((text) => context.measureText(text).width))) + 8;
      const top = 12;
      const plotWidth = width - left - 8;
      const plotHeight = height - top - 28;

      context.strokeStyle = "rgba(148, 163, 184, .14)";
      context.fillStyle = "rgb(148, 163, 184)";
      for (let index = 0; index <= 3; index += 1) {
        const y = top + (plotHeight * index) / 3;
        context.beginPath();
        context.moveTo(left, y);
        context.lineTo(width - 8, y);
        context.stroke();
        context.fillText(tickLabels[index], 0, y + 4);
      }

      [{ values, color, label }, ...additionalSeries].forEach((series, seriesIndex) => {
        context.strokeStyle = series.color;
        context.lineWidth = 2;
        context.beginPath();
        series.values.forEach((value, index) => {
          const x = left + (plotWidth * index) / Math.max(1, series.values.length - 1);
          const y = top + plotHeight * (1 - value / maximum);
          if (index === 0) context.moveTo(x, y);
          else context.lineTo(x, y);
        });
        context.stroke();

        const legendX = left + seriesIndex * 90;
        context.fillStyle = series.color;
        context.fillRect(legendX, height - 10, 10, 3);
        context.fillStyle = "rgb(148, 163, 184)";
        context.fillText(series.label, legendX + 15, height - 6);
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
      transferCards.forEach((card) => {
        const type = card.dataset.transferMetric;
        const values = samples.map((sample) => {
          const bytesIn = sample.ingest_bytes || 0;
          const bytesOut = sample.egress_bytes || 0;
          if (type === "in") return bytesIn;
          if (type === "out") return bytesOut;
          return bytesIn + bytesOut;
        });
        card.querySelector("[data-transfer-value]").textContent = formatBytes(values.at(-1) || 0);
        draw(
          card.querySelector("canvas"),
          values,
          type === "in" ? "rgb(74, 222, 128)" : type === "out" ? "rgb(56, 189, 248)" : "rgb(251, 191, 36)",
          type === "in" ? "Bytes in" : type === "out" ? "Bytes out" : "Total bytes",
          formatBytes,
          1_000,
        );
      });
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
      chatOutcomeCards.forEach((card) => {
        const webhooks = card.dataset.chatOutcomes === "webhooks";
        const successKey = webhooks ? "webhooks_received" : "automations_executed";
        const failureKey = webhooks ? "webhooks_rejected" : "automations_failed";
        const latest = samples.at(-1);
        card.querySelector("[data-chat-success]").textContent = (latest?.[successKey] || 0).toLocaleString();
        card.querySelector("[data-chat-failure]").textContent = (latest?.[failureKey] || 0).toLocaleString();
        draw(
          card.querySelector("canvas"),
          samples.map((sample) => sample[successKey] || 0),
          "rgb(74, 222, 128)",
          webhooks ? "Received" : "Executed",
          (value) => Math.round(value).toLocaleString(),
          1,
          [{
            values: samples.map((sample) => sample[failureKey] || 0),
            color: "rgb(248, 113, 113)",
            label: webhooks ? "Rejected" : "Failed",
          }],
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

    window.addEventListener("resize", render, { signal: abort.signal });

    const events = new EventSource("/api/metrics/events");
    abort.signal.addEventListener("abort", () => events.close(), { once: true });
    events.addEventListener("metrics_history", (event) => {
      samples = JSON.parse(event.data);
      render();
    });
    events.addEventListener("metrics_sample", (event) => {
      const sample = JSON.parse(event.data);
      if (!sample) return;
      if (samples.at(-1)?.timestamp_ms !== sample.timestamp_ms) {
        samples.push(sample);
      }
      if (samples.length > 300) samples.shift();
      render();
    });

    render();
  }

  disconnectedCallback() {
    this.observer?.disconnect();
    this.observer = null;
    this.cleanup?.();
    this.cleanup = null;
  }
});
