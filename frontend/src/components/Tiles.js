import { html } from "../html.js";
import { totals } from "../store.js";
import { compact, fmt } from "../format.js";
import { cn } from "../lib/utils.js";
import { Spark } from "./Spark.js";
import { Card } from "./ui/card.js";

function Tile({ label, value, unit, sub, spark, format = fmt }) {
  // An M3 card: label-large heading, headline-medium value.
  return html`<${Card} class="grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-0.5 px-4 max-sm:last:odd:col-span-2">
    <div class="text-muted-foreground text-sm font-medium tracking-[0.00714em]">${label}</div>
    <div class=${cn("text-[28px] leading-9 font-normal tabular-nums max-sm:text-2xl max-sm:leading-8", value == null && "text-muted-foreground")}>
      ${value ?? "—"}${unit && value != null && html`<small class="text-muted-foreground ml-1 text-[13px] font-medium tracking-normal">${unit}</small>`}
    </div>
    <div class="text-muted-foreground text-xs tabular-nums">${sub}</div>
    <${Spark} vals=${spark} format=${format} label=${label} />
  <//>`;
}

const count = (label, x, format) =>
  html`<${Tile} label=${label} value=${format(x.today)} unit="today" sub=${`${format(x.all)} all-time`} spark=${x.spark} format=${format} />`;

export function Tiles() {
  const t = totals.value;
  if (!t) return null;
  return html`<section class="grid grid-cols-2 gap-3 sm:grid-cols-[repeat(auto-fit,minmax(180px,1fr))] sm:gap-4" aria-label="Totals">
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
        : html`<span class="flex flex-wrap gap-x-3 [&>span]:whitespace-nowrap [&_b]:font-mono [&_b]:font-semibold">
              <span>nnue avg <b class="text-primary">${t.knps.nnue != null ? fmt(t.knps.nnue) : "—"}</b></span>
              <span>hce avg <b class="text-hce">${t.knps.hce != null ? fmt(t.knps.hce) : "—"}</b></span>
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
