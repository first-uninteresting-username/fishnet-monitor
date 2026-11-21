import { html } from "../html.js";
import { variants } from "../store.js";
import { fmt } from "../format.js";
import { Empty, Panel } from "./ui.js";

export function Variants() {
  const v = variants.value;
  const total = v.reduce((s, x) => s + x.n, 0);
  const max = Math.max(1, ...v.map((x) => x.n));
  return html`<${Panel} title="Batches by variant" hint="last 7 days">
    ${v.length
      ? html`<div class="vbars">
            ${v.map(({ name, n, eval: ev }) => {
              const share = (100 * n) / total;
              return html`<div class="vbar">
                <span>${name}</span>
                <div class="track"><div class=${`fill ${ev}`} style=${`width: ${((100 * n) / max).toFixed(2)}%`}></div></div>
                <span class="n num">${fmt(n)}</span>
                <span class="p num">${share < 1 ? share.toFixed(1) : Math.round(share)}%</span>
              </div>`;
            })}
          </div>
          <div class="legend">
            <span><i style="background: var(--accent)"></i>nnue</span><span><i style="background: var(--hce)"></i>hce</span>
          </div>`
      : html`<${Empty}>No batches yet.<//>`}
  <//>`;
}
