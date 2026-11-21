(() => {
  const W = 85;
  const H = 127;
  const CX = 42;
  const TOP_MIN_Y = 4;
  const TOP_MAX_Y = 61;
  const NECK_TOP_Y = 62;
  const NECK_MID_Y = 63;
  const NECK_BOT_Y = 64;
  const BOT_MIN_Y = 65;
  const BOT_MAX_Y = 122;
  const CHAMBER_ROWS = TOP_MAX_Y - TOP_MIN_Y; // 57

  const halfWidth = (d) =>
    Math.round(1 + 36 * Math.pow(Math.max(0, Math.min(1, d / CHAMBER_ROWS)), 0.52));

  // Precompute chamber masks and top-chamber drain order.
  const bottomMask = new Uint8Array(W * H);
  const topCells = [];

  for (let y = TOP_MIN_Y; y <= TOP_MAX_Y; y++) {
    const hw = halfWidth(TOP_MAX_Y - y);
    for (let x = CX - hw; x <= CX + hw; x++) {
      topCells.push({ x, y });
    }
  }

  for (let y = BOT_MIN_Y; y <= BOT_MAX_Y; y++) {
    const hw = halfWidth(y - BOT_MIN_Y);
    for (let x = CX - hw; x <= CX + hw; x++) {
      bottomMask[y * W + x] = 1;
    }
  }

  // Sort topCells so the bottom-most (closest to neck) are at index 0,
  // and the top-most (outer edges first, slight center funnel) are at the end.
  // Keeping `topCells.slice(0, remaining)` drains from the top down with a subtle funnel.
  topCells.sort((a, b) => {
    const scoreA = (TOP_MAX_Y - a.y) + Math.abs(a.x - CX) * 0.22;
    const scoreB = (TOP_MAX_Y - b.y) + Math.abs(b.x - CX) * 0.22;
    if (scoreA !== scoreB) return scoreA - scoreB;
    return Math.abs(a.x - CX) - Math.abs(b.x - CX);
  });

  const MAX_GRAINS = topCells.length;

  // Deterministic sand palette (warm golden/amber tones).
  const SAND_COLORS = [
    [245, 197, 99, 255],  // bright sand
    [232, 174, 72, 255],  // mid sand
    [212, 148, 52, 255],  // warm shadow sand
  ];

  const grainColorIndex = (i) => ((i * 2654435761) >>> 0) % SAND_COLORS.length + 1;

  // Build static frame + glass pixel layer once.
  const staticPixels = new Uint8ClampedArray(W * H * 4);
  const setPixel = (buf, x, y, r, g, b, a) => {
    if (x < 0 || x >= W || y < 0 || y >= H) return;
    const idx = (y * W + x) * 4;
    buf[idx] = r;
    buf[idx + 1] = g;
    buf[idx + 2] = b;
    buf[idx + 3] = a;
  };

  // Top & bottom caps (4px tall).
  for (let x = 2; x <= W - 3; x++) {
    // Top cap
    setPixel(staticPixels, x, 0, 161, 161, 170, 230);
    setPixel(staticPixels, x, 1, 212, 212, 216, 245);
    setPixel(staticPixels, x, 2, 113, 113, 122, 245);
    setPixel(staticPixels, x, 3, 82, 82, 91, 245);
    // Bottom cap
    setPixel(staticPixels, x, H - 4, 161, 161, 170, 245);
    setPixel(staticPixels, x, H - 3, 212, 212, 216, 245);
    setPixel(staticPixels, x, H - 2, 113, 113, 122, 245);
    setPixel(staticPixels, x, H - 1, 63, 63, 70, 230);
  }

  // Subtle side pillars connecting top & bottom caps.
  for (let y = 4; y <= H - 5; y++) {
    setPixel(staticPixels, 3, y, 82, 82, 91, 160);
    setPixel(staticPixels, W - 4, y, 82, 82, 91, 160);
  }

  // Glass outline & subtle interior tint.
  const drawGlassRow = (y, hw) => {
    const left = CX - hw;
    const right = CX + hw;
    // Faint interior glass tint
    for (let x = left; x <= right; x++) {
      setPixel(staticPixels, x, y, 255, 255, 255, 14);
    }
    // Crisp 1px glass walls
    setPixel(staticPixels, left - 1, y, 228, 228, 231, 180);
    setPixel(staticPixels, right + 1, y, 228, 228, 231, 180);
    // Specular highlight along left curve
    if (hw >= 4) {
      setPixel(staticPixels, left + 1, y, 255, 255, 255, 55);
    }
  };

  for (let y = TOP_MIN_Y; y <= TOP_MAX_Y; y++) {
    drawGlassRow(y, halfWidth(TOP_MAX_Y - y));
  }
  for (let y = NECK_TOP_Y; y <= NECK_BOT_Y; y++) {
    setPixel(staticPixels, CX, y, 255, 255, 255, 18);
    setPixel(staticPixels, CX - 1, y, 228, 228, 231, 200);
    setPixel(staticPixels, CX + 1, y, 228, 228, 231, 200);
  }
  for (let y = BOT_MIN_Y; y <= BOT_MAX_Y; y++) {
    drawGlassRow(y, halfWidth(y - BOT_MIN_Y));
  }

  const settleGrain = (grid, startX, startY, colorIdx, seed) => {
    let x = startX;
    let y = startY;
    // Find first open cell at or below startY
    while (y <= BOT_MAX_Y && grid[y * W + x] !== 0) {
      y++;
    }
    if (y > BOT_MAX_Y) {
      // Column full; scan nearby in bottom chamber for an open cell
      for (let ry = BOT_MIN_Y; ry <= BOT_MAX_Y; ry++) {
        for (let dx = 0; dx <= 37; dx++) {
          const lx = CX - dx;
          const rx = CX + dx;
          if (bottomMask[ry * W + lx] && grid[ry * W + lx] === 0) {
            grid[ry * W + lx] = colorIdx;
            return;
          }
          if (bottomMask[ry * W + rx] && grid[ry * W + rx] === 0) {
            grid[ry * W + rx] = colorIdx;
            return;
          }
        }
      }
      return;
    }

    // Roll down until resting
    let dirPref = seed & 1 ? -1 : 1;
    while (y < BOT_MAX_Y) {
      const below = (y + 1) * W + x;
      if (bottomMask[below] && grid[below] === 0) {
        y++;
        continue;
      }
      const d1x = x + dirPref;
      const d2x = x - dirPref;
      const d1 = (y + 1) * W + d1x;
      const d2 = (y + 1) * W + d2x;
      if (bottomMask[d1] && grid[d1] === 0) {
        x = d1x;
        y++;
        continue;
      }
      if (bottomMask[d2] && grid[d2] === 0) {
        x = d2x;
        y++;
        continue;
      }
      break;
    }
    grid[y * W + x] = colorIdx;
  };

  const instances = new Map();

  const createHourglass = (canvas) => {
    const startedAt = Number(canvas.dataset.startedAt || 0);
    const endsAt = Number(canvas.dataset.endsAt || 0);
    if (!Number.isFinite(startedAt) || !Number.isFinite(endsAt) || endsAt <= startedAt) {
      return null;
    }

    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;

    const totalSecs = Math.max(1, Math.round((endsAt - startedAt) / 1000));
    const totalGrains = Math.min(totalSecs, MAX_GRAINS);
    const grainsForElapsed = (elapsedSecs) => {
      const clamped = Math.min(totalSecs, Math.max(0, elapsedSecs));
      if (totalSecs <= MAX_GRAINS) return clamped;
      return Math.min(totalGrains, Math.floor((clamped / totalSecs) * totalGrains));
    };

    const bottomGrid = new Uint8Array(W * H);
    const falling = [];
    const frameImage = ctx.createImageData(W, H);

    let released = grainsForElapsed(Math.floor((Date.now() - startedAt) / 1000));
    for (let i = 1; i <= released; i++) {
      settleGrain(bottomGrid, CX, BOT_MIN_Y, grainColorIndex(i), i);
    }

    let lastTime = performance.now();
    let rafId = 0;
    let alive = true;

    const render = (nowPerf) => {
      if (!alive || !canvas.isConnected) {
        destroyHourglass(canvas);
        return;
      }

      const dt = Math.min(0.05, Math.max(0.001, (nowPerf - lastTime) / 1000));
      lastTime = nowPerf;

      const elapsedSecs = Math.floor((Date.now() - startedAt) / 1000);
      const targetReleased = grainsForElapsed(elapsedSecs);

      if (targetReleased > released) {
        const delta = targetReleased - released;
        // If tab was backgrounded, settle older grains immediately and animate the latest one.
        const immediate = delta > 3 ? delta - 1 : 0;
        for (let k = 0; k < immediate; k++) {
          released++;
          settleGrain(bottomGrid, CX, BOT_MIN_Y, grainColorIndex(released), released);
        }
        while (released < targetReleased) {
          released++;
          falling.push({
            x: CX,
            y: NECK_TOP_Y,
            vy: 28,
            color: grainColorIndex(released),
            seed: released,
          });
        }
      }

      // Advance falling pixels
      for (let i = falling.length - 1; i >= 0; i--) {
        const p = falling[i];
        p.vy += 190 * dt;
        const nextY = p.y + p.vy * dt;
        let landed = false;

        const stepEnd = Math.min(BOT_MAX_Y, Math.floor(nextY));
        for (let cy = Math.max(BOT_MIN_Y, Math.floor(p.y)); cy <= stepEnd; cy++) {
          if (bottomGrid[cy * W + p.x] !== 0) {
            settleGrain(bottomGrid, p.x, Math.max(BOT_MIN_Y, cy - 1), p.color, p.seed);
            landed = true;
            break;
          }
        }

        if (!landed && nextY >= BOT_MAX_Y) {
          settleGrain(bottomGrid, p.x, BOT_MAX_Y, p.color, p.seed);
          landed = true;
        }

        if (landed) {
          falling.splice(i, 1);
        } else {
          p.y = nextY;
        }
      }

      // Draw frame
      const data = frameImage.data;
      data.set(staticPixels);

      // Top chamber remaining sand
      const remaining = Math.max(0, totalGrains - released);
      for (let i = 0; i < remaining; i++) {
        const cell = topCells[i];
        const c = SAND_COLORS[grainColorIndex(totalGrains - i) - 1];
        setPixel(data, cell.x, cell.y, c[0], c[1], c[2], c[3]);
      }

      // Bottom chamber settled sand
      for (let y = BOT_MIN_Y; y <= BOT_MAX_Y; y++) {
        const rowOff = y * W;
        for (let x = 0; x < W; x++) {
          const ci = bottomGrid[rowOff + x];
          if (ci !== 0) {
            const c = SAND_COLORS[ci - 1];
            setPixel(data, x, y, c[0], c[1], c[2], c[3]);
          }
        }
      }

      // Falling pixels
      for (let i = 0; i < falling.length; i++) {
        const p = falling[i];
        const c = SAND_COLORS[p.color - 1];
        setPixel(data, p.x, Math.round(p.y), c[0], c[1], c[2], c[3]);
      }

      ctx.putImageData(frameImage, 0, 0);
      rafId = window.requestAnimationFrame(render);
    };

    rafId = window.requestAnimationFrame(render);

    return {
      key: `${startedAt}:${endsAt}`,
      stop() {
        alive = false;
        window.cancelAnimationFrame(rafId);
      },
    };
  };

  const destroyHourglass = (canvas) => {
    const inst = instances.get(canvas);
    if (inst) {
      inst.stop();
      instances.delete(canvas);
    }
  };

  const pad = (value) => String(value).padStart(2, "0");
  const lastShownSecs = new WeakMap();
  const tickCountdowns = () => {
    document.querySelectorAll("[data-pomodoro-countdown]").forEach((el) => {
      const endsAt = Number(el.dataset.endsAt || 0);
      if (!Number.isFinite(endsAt) || endsAt <= 0) return;
      let secs = Math.max(0, Math.floor((endsAt - Date.now()) / 1000));
      const prev = lastShownSecs.get(el);
      if (prev && prev.endsAt === endsAt && secs > prev.secs) {
        secs = prev.secs;
      }
      lastShownSecs.set(el, { endsAt, secs });
      const text = `${pad(Math.floor(secs / 60))}:${pad(secs % 60)}`;
      if (el.textContent !== text) el.textContent = text;
    });
  };

  const syncHourglasses = () => {
    const active = new Set(document.querySelectorAll("canvas[data-pomodoro-hourglass]"));
    for (const [canvas] of instances) {
      if (!active.has(canvas)) {
        destroyHourglass(canvas);
      }
    }
    active.forEach((canvas) => {
      const key = `${canvas.dataset.startedAt || 0}:${canvas.dataset.endsAt || 0}`;
      const existing = instances.get(canvas);
      if (existing && existing.key === key) return;
      if (existing) destroyHourglass(canvas);
      const inst = createHourglass(canvas);
      if (inst) instances.set(canvas, inst);
    });
  };

  const init = () => {
    syncHourglasses();
    tickCountdowns();
    if (document.body) {
      new MutationObserver(() => {
        syncHourglasses();
        tickCountdowns();
      }).observe(document.body, { childList: true, subtree: true, characterData: true });
    }
    window.setInterval(tickCountdowns, 1000);
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();
