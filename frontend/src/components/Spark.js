// Tile sparkline: drawn at its real pixel size (no stretched strokes), with a
// smooth line, a dot on the latest point, and a hover/touch/arrow-key readout.
// The series carries no timestamps; its points are taken as evenly spaced over
// the last `hours` hours, the latest at the right edge.
import { useEffect, useRef, useState } from "preact/hooks";
import { html } from "../html.js";
import { cn } from "../lib/utils.js";

// Floating readout box, shared with the positions chart.
export const TIP =
  "bg-popover text-popover-foreground pointer-events-none absolute z-10 grid gap-px rounded-md border px-2.5 py-1.5 text-xs whitespace-nowrap shadow-md";

const H = 48;
const PAD = { t: 6, b: 4, x: 4 };

// Monotone cubic (Fritsch–Carlson) through the points: smooth, never overshoots.
function smooth(pts) {
  const n = pts.length;
  if (n < 3) return `M${pts.map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join("L")}`;
  const dx = [], m = [];
  for (let i = 0; i < n - 1; i++) {
    dx[i] = pts[i + 1][0] - pts[i][0];
    m[i] = (pts[i + 1][1] - pts[i][1]) / dx[i];
  }
  const tan = [m[0]];
  for (let i = 1; i < n - 1; i++) tan[i] = m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2;
  tan[n - 1] = m[n - 2];
  for (let i = 0; i < n - 1; i++) {
    if (m[i] === 0) { tan[i] = tan[i + 1] = 0; continue; }
    const a = tan[i] / m[i], b = tan[i + 1] / m[i], s = a * a + b * b;
    if (s > 9) { const k = 3 / Math.sqrt(s); tan[i] = k * a * m[i]; tan[i + 1] = k * b * m[i]; }
  }
  let d = `M${pts[0][0].toFixed(1)},${pts[0][1].toFixed(1)}`;
  for (let i = 0; i < n - 1; i++) {
    const h = dx[i] / 3;
    d += `C${(pts[i][0] + h).toFixed(1)},${(pts[i][1] + tan[i] * h).toFixed(1)} ${(pts[i + 1][0] - h).toFixed(1)},${(pts[i + 1][1] - tan[i + 1] * h).toFixed(1)} ${pts[i + 1][0].toFixed(1)},${pts[i + 1][1].toFixed(1)}`;
  }
  return d;
}

const ago = (hrs) => (hrs < 0.05 ? "now" : hrs < 1 ? `${Math.round(hrs * 60)} min ago` : `${hrs.toFixed(hrs < 10 ? 1 : 0).replace(/\.0$/, "")} h ago`);

export function Spark({ vals, format = String, label = "", hours = 24, color = "var(--primary)" }) {
  const box = useRef();
  const [w, setW] = useState(0);
  const [hot, setHot] = useState(null);

  useEffect(() => {
    const el = box.current;
    const ro = new ResizeObserver(() => setW(Math.round(el.clientWidth)));
    ro.observe(el);
    setW(Math.round(el.clientWidth));
    return () => ro.disconnect();
  }, []);

  const n = vals?.length ?? 0;
  const ok = n > 1 && w > 0;
  let mn = 0, mx = 1, x = () => 0, y = () => 0, path = "", pts = [];
  if (ok) {
    mn = Math.min(...vals);
    mx = Math.max(...vals);
    const span = mx - mn || Math.abs(mx) || 1;
    const lo = mn - span * 0.08;
    const hi = mx + span * 0.08;
    x = (i) => PAD.x + (i / (n - 1)) * (w - 2 * PAD.x);
    y = (v) => PAD.t + (1 - (v - lo) / (hi - lo)) * (H - PAD.t - PAD.b);
    pts = vals.map((v, i) => [x(i), y(v)]);
    path = smooth(pts);
  }

  const at = (clientX) => {
    const r = box.current.getBoundingClientRect();
    const f = (clientX - r.left - PAD.x) / (r.width - 2 * PAD.x);
    setHot(Math.min(n - 1, Math.max(0, Math.round(f * (n - 1)))));
  };
  const onKey = (e) => {
    if (e.key === "Escape") return setHot(null);
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const d = (e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 5 : 1);
    setHot((h) => Math.min(n - 1, Math.max(0, (h ?? n - 1) + d)));
  };

  const last = n - 1;
  const cur = hot ?? last;
  const tipX = ok ? Math.min(Math.max(x(cur), 46), w - 46) : 0;
  const gid = `g${label.replace(/\W/g, "")}`;

  return html`<div
    class="focus-visible:outline-ring relative -mx-1 mt-1 h-12 cursor-crosshair touch-pan-y rounded-md outline-offset-2 focus-visible:outline-2"
    ref=${box}
    tabindex=${ok ? 0 : -1}
    role="img"
    aria-label=${`${label} over the last ${hours} hours${ok ? `: latest ${format(vals[last])}, low ${format(mn)}, high ${format(mx)}. Arrow keys read values.` : ": no data yet"}`}
    onPointerMove=${(e) => ok && at(e.clientX)}
    onPointerDown=${(e) => ok && at(e.clientX)}
    onPointerLeave=${() => setHot(null)}
    onBlur=${() => setHot(null)}
    onKeyDown=${onKey}
  >
    ${ok &&
    html`<svg class="block overflow-visible" width=${w} height=${H} viewBox=${`0 0 ${w} ${H}`} aria-hidden="true">
        <defs>
          <linearGradient id=${gid} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color=${color} stop-opacity=".28" />
            <stop offset="1" stop-color=${color} stop-opacity="0" />
          </linearGradient>
        </defs>
        <path d=${`${path}L${x(last).toFixed(1)},${H}L${x(0).toFixed(1)},${H}Z`} fill=${`url(#${gid})`} />
        <path d=${path} fill="none" stroke=${color} stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" />
        ${hot != null && html`<line class="stroke-muted-foreground [stroke-dasharray:2_2]" stroke-width="1" x1=${x(hot)} x2=${x(hot)} y1="0" y2=${H} />`}
        <circle class="stroke-card" stroke-width="2" cx=${x(cur)} cy=${y(vals[cur])} r=${hot != null ? 4 : 3} fill=${color} />
      </svg>
      ${hot != null &&
      html`<div class=${cn(TIP, "top-0 -translate-x-1/2 -translate-y-[calc(100%+6px)] px-2 py-1")} style=${`left:${tipX}px`}>
        <b class="text-base font-semibold tabular-nums">${format(vals[hot])}</b>
        <span class="text-muted-foreground font-mono text-[11px]">${ago(((last - hot) / last) * hours)}</span>
      </div>`}`}
  </div>`;
}
