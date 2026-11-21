// The dashboard page. Data sources write the signals in store.js; this file
// only lays out the panels.
import { html } from "./html.js";
import { lookup } from "./store.js";
import { stale } from "./derived.js";
import { FatalStrip, Header, StallStrip, StaleStrip } from "./components/Header.js";
import { Tiles } from "./components/Tiles.js";
import { PositionsChart } from "./components/PositionsChart.js";
import { Variants } from "./components/Variants.js";
import { Problems } from "./components/Problems.js";
import { PlayerStats, TopGames } from "./components/Players.js";
import { Batches } from "./components/Batches.js";

function Footer() {
  return html`<footer>
    <span>Reads fishnet's log and <code>/data/fishnet-stats</code>. Never shows your key.</span>
    <span>
      ${lookup.value.enabled
        ? html`Player data from <code>lichess.org/api/games/export/_ids</code> (turn off with <code>GAME_LOOKUP=0</code>).`
        : html`Game lookup is off (<code>GAME_LOOKUP=0</code>), so the player panels and columns are hidden.`}
    </span>
    <span><code>/metrics</code> · <code>/api/status</code> · <code>/healthz</code></span>
  </footer>`;
}

// `top` is rendered above the header (the demo puts its controls there).
export function App({ top }) {
  return html`<div class="wrap">
    ${top}
    <${Header} />
    <${StallStrip} />
    <${FatalStrip} />
    <${StaleStrip} />
    <div class=${`content${stale.value ? " stale" : ""}`}>
      <${Tiles} />
      <${PositionsChart} />
      <div class="grid2">
        <${Variants} />
        <${Problems} />
      </div>
      ${lookup.value.enabled &&
      html`<div class="grid2">
        <${TopGames} />
        <${PlayerStats} />
      </div>`}
      <${Batches} />
    </div>
    <${Footer} />
  </div>`;
}
