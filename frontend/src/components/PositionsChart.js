// Positions per minute: uPlot, with idle/stopped stretches shaded, an average
// line for the visible window, a tooltip (mouse, touch, arrow keys), drag to
// zoom, double-click or Esc to reset. New data goes in through setData; the
// chart is only rebuilt when the colour scheme changes.
import uPlot from "uplot";
import { useEffect, useRef, useState } from "preact/hooks";
import { render } from "preact";
import { useSignal } from "@preact/signals";
import { html } from "../html.js";
import { cn } from "../lib/utils.js";
import { positions } from "../store.js";
import { dayLabel, hm, minutes } from "../format.js";
import { Empty, Legend, Note, Panel, Seg } from "./common.js";
import { Button } from "./ui/button.js";
import { TIP } from "./Spark.js";

const RANGES = [["1h", "1 h"], ["6h", "6 h"], ["24h", "24 h"], ["7d", "7 d"]];
const MIN_ZOOM_BUCKETS = 4;
const FONT = '10px ui-monospace, "SFMono-Regular", "Cascadia Mono", Menlo, Consolas, monospace';

// Indices of the buckets inside [t0, t1] (seconds); the whole series without a view.
function visibleRange(d, view) {
  if (!view) return [0, d.t.length - 1];
  let lo = d.t.findIndex((t) => t >= view[0]);
  let hi = d.t.findLastIndex((t) => t <= view[1]);
  if (lo < 0 || hi < lo) lo = hi = Math.max(0, hi);
  return [lo, hi];
}

function stats(d, view) {
  const [lo, hi] = visibleRange(d, view);
  let busySum = 0, busyN = 0, idleN = 0, zeroN = 0, peak = lo;
  for (let i = lo; i <= hi; i++) {
    if (d.v[i] > 0) { busySum += d.v[i]; busyN++; }
    if (d.kind[i] === "idle") idleN++;
    if (d.v[i] === 0 && d.kind[i] !== "quiet") zeroN++;
    if (d.v[i] > d.v[peak]) peak = i;
  }
  const n = hi - lo + 1;
  return {
    avg: busyN ? busySum / busyN : 0,
    busyPct: Math.round(100 * (1 - zeroN / n)),
    peak: { v: d.v[peak], t: d.t[peak] },
    idleMin: (idleN * d.step) / 60,
  };
}

function summary(d, view) {
  const s = stats(d, view);
  const span = view ? `${hm(view[0] * 1000)}–${hm(view[1] * 1000)} · ` : "";
  const idle = s.idleMin ? ` · idle ${minutes(s.idleMin)}` : "";
  return `${span}${s.busyPct}% busy · avg ${Math.round(s.avg)} · peak ${Math.round(s.peak.v)} at ${hm(s.peak.t * 1000)}${idle}`;
}

// ---------- colours (canvas can't read CSS variables) ----------

function rgba(hex, a) {
  let h = hex.replace("#", "");
  if (h.length <= 4) h = [...h].map((c) => c + c).join("");
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

function palette() {
  const css = getComputedStyle(document.documentElement);
  const get = (n) => css.getPropertyValue(`--${n}`).trim();
  return {
    accent: get("primary"),
    surface: get("card"),
    fg: get("foreground"),
    muted: get("muted-foreground"),
    line: get("border"),
    idle: get("idle"),
    crit: get("crit"),
  };
}

// Re-render when the OS switches between light and dark.
function useColorScheme() {
  const [n, setN] = useState(0);
  useEffect(() => {
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setN((x) => x + 1);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return n;
}

// ---------- uPlot hooks ----------

function drawBands(u, s, c) {
  const { t, kind, step } = s.data;
  const { ctx, bbox } = u;
  ctx.save();
  ctx.beginPath();
  ctx.rect(bbox.left, bbox.top, bbox.width, bbox.height);
  ctx.clip();
  for (const [k, fill] of [["idle", rgba(c.idle, 0.2)], ["stop", rgba(c.crit, 0.3)]]) {
    ctx.fillStyle = fill;
    for (let i = 0; i < t.length; i++) {
      if (kind[i] !== k) continue;
      let j = i;
      while (j + 1 < t.length && kind[j + 1] === k) j++;
      const x0 = u.valToPos(t[i] - step / 2, "x", true);
      const x1 = u.valToPos(t[j] + step / 2, "x", true);
      ctx.fillRect(x0, bbox.top, x1 - x0, bbox.height);
      i = j;
    }
  }
  ctx.restore();
}

function drawOverlay(u, s, c) {
  const { ctx, bbox } = u;
  const pr = uPlot.pxRatio;
  const { t, v } = s.data;
  s.avg = stats(s.data, [u.scales.x.min, u.scales.x.max]).avg;
  ctx.save();
  if (s.avg) {
    const y = u.valToPos(s.avg, "y", true);
    ctx.strokeStyle = rgba(c.fg, 0.4);
    ctx.lineWidth = pr;
    ctx.setLineDash([4 * pr, 4 * pr]);
    ctx.beginPath();
    ctx.moveTo(bbox.left, y);
    ctx.lineTo(bbox.left + bbox.width, y);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = c.muted;
    ctx.font = FONT.replace("10px", `${10 * pr}px`);
    ctx.textAlign = "right";
    ctx.fillText(`avg ${Math.round(s.avg)}`, bbox.left + bbox.width - 2 * pr, y - 5 * pr);
  }
  const last = t.length - 1;
  if (t[last] <= u.scales.x.max) {
    ctx.beginPath();
    ctx.arc(u.valToPos(t[last], "x", true), u.valToPos(v[last], "y", true), 3.5 * pr, 0, 2 * Math.PI);
    ctx.fillStyle = c.accent;
    ctx.strokeStyle = c.surface;
    ctx.lineWidth = 2 * pr;
    ctx.fill();
    ctx.stroke();
  }
  ctx.restore();
}

function Tip({ s, i }) {
  const { t, v, kind, step } = s.data;
  const end = t[i] * 1000;
  const span = step === 60 ? hm(end) : `${hm(end - step * 1000)}–${hm(end)}`;
  const diff = v[i] - s.avg;
  const cmp =
    kind[i] === "stop" ? html`<span class="text-crit text-[11.5px]">stopped · fishnet not running</span>`
    : kind[i] === "quiet" ? html`<span class="text-warn text-[11.5px]">no batch and no idle line</span>`
    : kind[i] === "idle" ? html`<span class="text-muted-foreground text-[11.5px]">idle · no work from lichess</span>`
    : html`<span class=${`text-[11.5px] ${diff >= 0 ? "text-ok" : "text-warn"}`}>${diff >= 0 ? "+" : "−"}${Math.abs(Math.round(diff))} vs avg</span>`;
  return html`<span class="text-muted-foreground font-mono text-[11px]">${dayLabel(end)} · ${span}</span>
    <b class="text-base font-semibold tabular-nums">
      ${Math.round(v[i])}<small class="text-muted-foreground ml-1 text-[11.5px] font-medium">positions/min</small>
    </b>
    ${cmp}`;
}

function moveTip(u, s, tip, host) {
  const i = u.cursor.idx;
  if (i == null || u.cursor.left < 0) {
    tip.hidden = true;
    return;
  }
  render(html`<${Tip} s=${s} i=${i} />`, tip);
  tip.hidden = false;
  const offset = u.over.getBoundingClientRect().left - host.getBoundingClientRect().left;
  const x = offset + u.valToPos(s.data.t[i], "x");
  const w = tip.offsetWidth;
  tip.style.left = `${x + 14 + w > host.clientWidth ? x - 14 - w : x + 14}px`;
}

// Keep a drag-zoom at least MIN_ZOOM_BUCKETS wide, and report the view.
function onScale(u, s) {
  const { t, step } = s.data;
  let { min, max } = u.scales.x;
  if (max - min < MIN_ZOOM_BUCKETS * step && t.length > MIN_ZOOM_BUCKETS) {
    const mid = (min + max) / 2;
    min = Math.max(t[0], mid - (MIN_ZOOM_BUCKETS * step) / 2);
    max = Math.min(t[t.length - 1], min + MIN_ZOOM_BUCKETS * step);
    u.setScale("x", { min, max });
    return;
  }
  const full = min <= t[0] && max >= t[t.length - 1];
  s.onView(full ? null : [min, max]);
}

const size = (el) => ({ width: Math.max(280, el.clientWidth), height: el.clientWidth < 520 ? 210 : 250 });

function options(el, s, c, tip) {
  return {
    ...size(el),
    padding: [12, 12, 0, 0],
    legend: { show: false },
    cursor: {
      y: false,
      drag: { x: true, y: false, dist: 8 },
      points: { size: 9, width: 2, fill: c.accent, stroke: c.surface },
    },
    scales: {
      x: { time: true },
      y: { range: (u, min, max) => [0, Math.max(10, max) * 1.08] },
    },
    axes: [
      { stroke: c.muted, font: FONT, space: 80, grid: { show: false }, ticks: { stroke: c.line, width: 1, size: 4 } },
      { stroke: c.muted, font: FONT, size: 36, gap: 4, grid: { stroke: c.line, width: 1 }, ticks: { show: false } },
    ],
    series: [{}, { stroke: c.accent, width: 1.6, fill: rgba(c.accent, 0.14), points: { show: false } }],
    hooks: {
      drawClear: [(u) => drawBands(u, s, c)],
      draw: [(u) => drawOverlay(u, s, c)],
      setCursor: [(u) => moveTip(u, s, tip, el)],
      setScale: [(u, key) => key === "x" && onScale(u, s)],
    },
  };
}

function Plot({ data, zoomed, onView, api }) {
  const host = useRef();
  const tip = useRef();
  // Mutable state the uPlot hooks read; refreshed on every render.
  const s = useRef({ avg: 0 }).current;
  s.data = data;
  s.onView = onView;
  const scheme = useColorScheme();

  useEffect(() => {
    const el = host.current;
    const u = new uPlot(options(el, s, palette(), tip.current), [s.data.t, s.data.v], el);
    api.current = u;
    const ro = new ResizeObserver(() => u.setSize(size(el)));
    ro.observe(el);
    // Touch has no hover: show the value under the finger instead.
    const touch = (e) => {
      if (e.pointerType === "mouse") return;
      const r = u.over.getBoundingClientRect();
      u.setCursor({ left: e.clientX - r.left, top: e.clientY - r.top });
    };
    u.over.addEventListener("pointerdown", touch);
    u.over.addEventListener("pointermove", touch);
    return () => {
      ro.disconnect();
      u.destroy();
      api.current = null;
    };
  }, [scheme]);

  // New data keeps the zoom; a new range (zoomed is false) resets it.
  useEffect(() => {
    api.current?.setData([data.t, data.v], !zoomed);
  }, [data]);

  const onKey = (e) => {
    const u = api.current;
    if (!u) return;
    if (e.key === "Escape") {
      u.setScale("x", { min: data.t[0], max: data.t[data.t.length - 1] });
      u.setCursor({ left: -10, top: -10 });
      return;
    }
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const [lo, hi] = visibleRange(data, [u.scales.x.min, u.scales.x.max]);
    const step = (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 10 : 1);
    const i = u.cursor.idx == null || u.cursor.left < 0 ? hi : Math.min(Math.max(u.cursor.idx + step, lo), hi);
    u.setCursor({ left: u.valToPos(data.t[i], "x"), top: u.valToPos(data.v[i], "y") });
  };

  return html`<div
    class="focus-visible:outline-ring relative min-h-[210px] cursor-crosshair touch-pan-y rounded-md outline-offset-2 select-none focus-visible:outline-2 [&_.u-cursor-x]:border-r [&_.u-cursor-x]:border-solid [&_.u-cursor-x]:border-muted-foreground [&_.u-select]:border-x [&_.u-select]:border-dashed [&_.u-select]:border-primary/50 [&_.u-select]:bg-primary/15"
    ref=${host}
    tabindex="0"
    role="img"
    aria-label="Positions per minute. Use the left and right arrow keys to read values, Escape to reset the zoom."
    onKeyDown=${onKey}
    onBlur=${() => api.current?.setCursor({ left: -10, top: -10 })}
  >
    <div class=${cn(TIP, "top-1")} ref=${tip} hidden></div>
  </div>`;
}

export function PositionsChart() {
  const range = useSignal("24h");
  const view = useSignal(null);
  const api = useRef(null);
  const data = positions.value[range.value];
  const empty = !data || data.t.length === 0;

  const pick = (r) => {
    view.value = null;
    range.value = r;
  };
  const reset = () => data && api.current?.setScale("x", { min: data.t[0], max: data.t[data.t.length - 1] });

  return html`<${Panel}
    title="Positions per minute"
    hint=${empty ? "" : summary(data, view.value)}
    actions=${html`
      ${view.value && html`<${Button} variant="outline" size="sm" class="h-7 text-xs" onClick=${reset}>Reset zoom<//>`}
      <${Seg} label="Range" options=${RANGES} value=${range.value} onChange=${pick} />
    `}
  >
    ${empty
      ? html`<${Empty} title="Collecting data.">The first point appears one minute after the first batch.<//>`
      : html`
          <${Plot} data=${data} zoomed=${!!view.value} onView=${(v) => (view.value = v)} api=${api} />
          <${Legend}
            items=${[
              ["bg-primary", "positions analysed"],
              ["bg-idle/30", "idle (no work from lichess)"],
              ["bg-crit/35", "stopped"],
            ]}
          />
          <${Note}>
            Hover, tap or use the arrow keys to read a value. Drag across the chart to zoom in; double-click or press Esc
            to zoom back out.
          <//>
        `}
  <//>`;
}
