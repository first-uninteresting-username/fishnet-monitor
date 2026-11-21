import { useSignal } from "@preact/signals";
import { html } from "../html.js";
import { now, problems } from "../store.js";
import { ago, stamp } from "../format.js";
import { cn } from "../lib/utils.js";
import { Empty, Note, Panel, Seg } from "./common.js";

const FILTERS = [["all", "All"], ["E", "Errors"], ["W", "Warnings"]];

function Problem({ p, at }) {
  const when =
    p.n > 1
      ? `first ${stamp(p.first, at)} · last ${stamp(p.last, at)} (${ago(at - p.last)})`
      : `${stamp(p.last, at)} (${ago(at - p.last)})`;
  const fatal = p.lvl === "F";
  return html`<li
    class=${cn(
      "grid grid-cols-[auto_minmax(0,1fr)_auto] items-baseline gap-x-2.5 gap-y-1 rounded-md border-l-3 border-l-(--sev) px-2.5 py-2",
      fatal ? "bg-[color-mix(in_srgb,var(--crit)_12%,var(--card))]" : "bg-muted",
    )}
    style=${`--sev: var(${p.lvl === "W" ? "--warn" : "--crit"})`}
  >
    <span class="font-mono text-xs font-semibold text-(--sev)">${fatal ? "FATAL" : `${p.lvl}:`}</span>
    <span class=${cn("font-mono text-[12.5px] wrap-anywhere", fatal && "font-medium")}>${p.msg}</span>
    ${p.n > 1
      ? html`<span class="rounded-full bg-(--sev)/15 px-1.5 font-mono text-xs font-semibold text-(--sev) tabular-nums" title=${`${p.n} similar lines`}>×${p.n}</span>`
      : html`<span></span>`}
    <span class="text-muted-foreground col-start-2 col-end-[-1] text-[11.5px] tabular-nums">${when}</span>
  </li>`;
}

export function Problems() {
  const filter = useSignal("all");
  const all = problems.value;
  const f = filter.value;
  const list = all.filter((p) => f === "all" || p.lvl === f || (f === "E" && p.lvl === "F"));
  const lines = all.reduce((s, p) => s + p.n, 0);
  return html`<${Panel}
    title="Problems"
    hint=${all.length ? `${lines} lines in 24 h` : ""}
    actions=${html`<${Seg} label="Filter" options=${FILTERS} value=${f} onChange=${(v) => (filter.value = v)} />`}
  >
    ${list.length
      ? html`<ul class="grid gap-1.5">${list.map((p) => html`<${Problem} key=${p.msg} p=${p} at=${now.value} />`)}</ul>`
      : all.length
        ? html`<${Empty} title=${`No ${f === "E" ? "errors" : "warnings"}`}>in the last 24 h.<//>`
        : html`<${Empty} title="No warnings or errors">since the monitor started.<//>`}
    <${Note}>
      Lines that differ only in numbers (backoff times, batch IDs) are grouped. The message shown is the latest one.
    <//>
  <//>`;
}
