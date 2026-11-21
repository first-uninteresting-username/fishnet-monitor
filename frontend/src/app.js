// The dashboard page. Data sources write the signals in store.js; this file
// only lays out the panels.
import { html } from "./html.js";
import { cn } from "./lib/utils.js";
import { lookup } from "./store.js";
import { stale } from "./derived.js";
import { FatalStrip, Header, StallStrip, StaleStrip } from "./components/Header.js";
import { Tiles } from "./components/Tiles.js";
import { PositionsChart } from "./components/PositionsChart.js";
import { Variants } from "./components/Variants.js";
import { Problems } from "./components/Problems.js";
import { PlayerStats, TopGames } from "./components/Players.js";
import { Batches } from "./components/Batches.js";

// `top` is rendered above the header (the demo puts its controls there).
export function App({ top }) {
  return html`<div class="mx-auto grid max-w-[1240px] gap-4 pt-4 pb-10 pr-[max(16px,env(safe-area-inset-right,0px))] pl-[max(16px,env(safe-area-inset-left,0px))]">
    ${top}
    <${Header} />
    <${StallStrip} />
    <${FatalStrip} />
    <${StaleStrip} />
    <div class=${cn("grid gap-4", stale.value && "*:opacity-55 *:saturate-30")}>
      <${Tiles} />
      <${PositionsChart} />
      <div class="grid grid-cols-1 gap-4 min-[860px]:grid-cols-2">
        <${Variants} />
        <${Problems} />
      </div>
      ${lookup.value.enabled &&
      html`<div class="grid grid-cols-1 gap-4 min-[860px]:grid-cols-2">
        <${TopGames} />
        <${PlayerStats} />
      </div>`}
      <${Batches} />
    </div>
  </div>`;
}
