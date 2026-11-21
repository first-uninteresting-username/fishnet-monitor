import { useSignal } from "@preact/signals";
import { html } from "../html.js";
import { now, problems } from "../store.js";
import { ago, stamp } from "../format.js";
import { Empty, Panel, Seg } from "./ui.js";

const FILTERS = [["all", "All"], ["E", "Errors"], ["W", "Warnings"]];

function Problem({ p, at }) {
  const when =
    p.n > 1
      ? `first ${stamp(p.first, at)} · last ${stamp(p.last, at)} (${ago(at - p.last)})`
      : `${stamp(p.last, at)} (${ago(at - p.last)})`;
  return html`<li class=${p.lvl === "F" ? "fatal" : ""} style=${`--sev: var(${p.lvl === "W" ? "--warn" : "--crit"})`}>
    <span class="lvl">${p.lvl === "F" ? "FATAL" : `${p.lvl}:`}</span>
    <span class="msg">${p.msg}</span>
    ${p.n > 1 ? html`<span class="count num" title=${`${p.n} similar lines`}>×${p.n}</span>` : html`<span></span>`}
    <span class="when num">${when}</span>
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
      ? html`<ul class="problems">${list.map((p) => html`<${Problem} key=${p.msg} p=${p} at=${now.value} />`)}</ul>`
      : all.length
        ? html`<${Empty} title=${`No ${f === "E" ? "errors" : "warnings"}`}>in the last 24 h.<//>`
        : html`<${Empty} title="No warnings or errors">since the monitor started.<//>`}
    <div class="note">
      Lines that differ only in numbers (backoff times, batch IDs) are grouped. The message shown is the latest one.
    </div>
  <//>`;
}
