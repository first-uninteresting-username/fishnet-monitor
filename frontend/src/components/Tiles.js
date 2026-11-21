import { html } from "../html.js";
import { totals } from "../store.js";
import { compact, fmt } from "../format.js";

function Spark({ vals }) {
  if (!vals?.length) return html`<svg viewBox="0 0 200 28" preserveAspectRatio="none" aria-hidden="true"></svg>`;
  const mx = Math.max(...vals);
  const mn = Math.min(...vals) * 0.9;
  const p = vals.map((v, i) => `${(i / (vals.length - 1)) * 200},${(26 - ((v - mn) / (mx - mn || 1)) * 22).toFixed(1)}`);
  return html`<svg viewBox="0 0 200 28" preserveAspectRatio="none" aria-hidden="true">
    <path d=${`M0,28 L${p.join(" L")} L200,28 Z`} fill="var(--accent)" fill-opacity=".12" />
    <path d=${`M${p.join(" L")}`} fill="none" stroke="var(--accent)" stroke-width="1.4" vector-effect="non-scaling-stroke" />
  </svg>`;
}

function Tile({ label, value, unit, sub, spark }) {
  return html`<div class="tile">
    <div class="label">${label}</div>
    <div class=${`value num${value == null ? " none" : ""}`}>${value ?? "—"}${unit && value != null && html`<small>${unit}</small>`}</div>
    <div class="sub num">${sub}</div>
    <${Spark} vals=${spark} />
  </div>`;
}

const count = (label, x, format) =>
  html`<${Tile} label=${label} value=${format(x.today)} unit="today" sub=${`${format(x.all)} all-time`} spark=${x.spark} />`;

export function Tiles() {
  const t = totals.value;
  if (!t) return null;
  return html`<section class="tiles" aria-label="Totals">
    ${count("Batches", t.batches, fmt)}
    ${count("Positions", t.positions, compact)}
    ${count("Nodes", t.nodes, compact)}
    <${Tile}
      label="Speed"
      value=${t.knps.last != null ? fmt(t.knps.last) : null}
      unit="knps/core"
      spark=${t.knps.spark}
      sub=${t.knps.last == null
        ? "after the first batch"
        : html`<span class="split">
              <span class="n">nnue avg <b>${t.knps.nnue != null ? fmt(t.knps.nnue) : "—"}</b></span>
              <span class="h">hce avg <b>${t.knps.hce != null ? fmt(t.knps.hce) : "—"}</b></span>
            </span>
            24 h, by engine; last batch above`}
    />
    <${Tile}
      label="NNUE"
      value=${t.nnue.mnps != null ? t.nnue.mnps.toFixed(1) : null}
      unit="Mnps"
      sub=${t.nnue.mnps != null ? "from the 2-minute summary" : "after the first 2-minute summary"}
      spark=${t.nnue.spark}
    />
  </section>`;
}
