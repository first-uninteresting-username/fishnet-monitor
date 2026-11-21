import { html } from "../html.js";
import { variants } from "../store.js";
import { fmt } from "../format.js";
import { Bar, Empty, Legend, Panel } from "./common.js";

const FILL = { nnue: "bg-primary", hce: "bg-hce" };

export function Variants() {
  const v = variants.value;
  const total = v.reduce((s, x) => s + x.n, 0);
  const max = Math.max(1, ...v.map((x) => x.n));
  return html`<${Panel} title="Batches by variant" hint="last 7 days">
    ${v.length
      ? html`<div class="grid gap-1.5">
            ${v.map(({ name, n, eval: ev }) => {
              const share = (100 * n) / total;
              return html`<${Bar}
                label=${name}
                pct=${(100 * n) / max}
                fill=${FILL[ev]}
                n=${fmt(n)}
                share=${`${share < 1 ? share.toFixed(1) : Math.round(share)}%`}
              />`;
            })}
          </div>
          <${Legend} items=${[["bg-primary", "nnue"], ["bg-hce", "hce"]]} />`
      : html`<${Empty}>No batches yet.<//>`}
  <//>`;
}
