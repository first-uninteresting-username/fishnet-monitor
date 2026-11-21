import { html } from "../html.js";
import { totals } from "../store.js";
import { compact, fmt } from "../format.js";
import { Spark } from "./Spark.js";

function Tile({ label, value, unit, sub, spark, format = fmt }) {
  return html`<div class="tile">
    <div class="label">${label}</div>
    <div class=${`value num${value == null ? " none" : ""}`}>${value ?? "—"}${unit && value != null && html`<small>${unit}</small>`}</div>
    <div class="sub num">${sub}</div>
    <${Spark} vals=${spark} format=${format} label=${label} />
  </div>`;
}

const count = (label, x, format) =>
  html`<${Tile} label=${label} value=${format(x.today)} unit="today" sub=${`${format(x.all)} all-time`} spark=${x.spark} format=${format} />`;

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
      format=${(v) => `${fmt(Math.round(v))} knps`}
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
      format=${(v) => `${v.toFixed(1)} Mnps`}
    />
  </section>`;
}
