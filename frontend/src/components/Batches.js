import { html } from "../html.js";
import { batches, client, conn, lookup } from "../store.js";
import { quiet, stale } from "../derived.js";
import { fmt, hms } from "../format.js";
import { Empty, Panel } from "./ui.js";
import { GameLink, GameRating, Pair } from "./Players.js";

function Live() {
  const lost = conn.value.state === "lost";
  const on = !stale.value && client.value.state === "running" && !quiet.value && batches.value.length > 0;
  const tone = lost ? "warn" : on ? "ok" : "idle";
  return html`<span class=${`live${on ? " on" : ""}`} style=${`--tone: var(--${tone})`}>
    <span class="dot"></span><span>${lost ? "reconnecting…" : on ? "live" : "no new batches"}</span>
  </span>`;
}

function Row({ b, withPlayers, ratelimited }) {
  const pending = html`<span class="pending">${ratelimited ? "Waiting: lichess is rate-limiting lookups" : "Looking up players…"}</span>`;
  return html`<tr class=${b.live ? "new" : ""}>
    <td class="num mono top o1">${hms(b.at)}</td>
    ${withPlayers &&
    html`<td class="wide o3">${b.game ? html`<${Pair} game=${b.game} />` : pending}</td>
      <td class="r o4" data-label="Rating">${b.game ? html`<${GameRating} game=${b.game} />` : html`<span class="pending">…</span>`}</td>`}
    <td class="o5" data-label="Variant">${b.variant} <span class=${`chip ${b.eval}`}>${b.eval}</span></td>
    <td class="r num mono o6" data-label="knps/core">${fmt(b.knps)}</td>
    <td class="o2"><${GameLink} id=${b.id} title=${b.example && "Example ID, not a real game"} /></td>
  </tr>`;
}

export function Batches() {
  const list = batches.value;
  const l = lookup.value;
  return html`<${Panel} title="Recent batches" actions=${html`<${Live} />`}>
    ${list.length
      ? html`<div class="scroll">
          <table class="cards">
            <thead>
              <tr>
                <th>Time</th>
                ${l.enabled && html`<th>Players</th><th class="r">Rating</th>`}
                <th>Variant</th><th class="r">knps/core</th><th>Game</th>
              </tr>
            </thead>
            <tbody>
              ${list.map((b) => html`<${Row} key=${b.key} b=${b} withPlayers=${l.enabled} ratelimited=${l.state === "ratelimited"} />`)}
            </tbody>
          </table>
        </div>`
      : html`<${Empty} title="Waiting for the first batch.">
          fishnet asks lichess for work as soon as it starts. When lichess has nothing to analyse, the client goes idle and
          this stays empty.
        <//>`}
  <//>`;
}
