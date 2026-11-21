// Everything that needs the game lookup: player cells, top games, the
// titled/supporter panel and the lookup status line.
import { useSignal } from "@preact/signals";
import { html } from "../html.js";
import { lookup, now, players, topGames } from "../store.js";
import { fmt, hms, share, when } from "../format.js";
import { Empty, Panel, Seg, Wing } from "./ui.js";

export function Player({ p, side }) {
  if (p.ai) {
    return html`<span class="pl ai"><i class=${`side ${side}`}></i><span class="nm">Stockfish level ${p.ai}</span></span>`;
  }
  return html`<span class="pl">
    <i class=${`side ${side}`}></i>
    ${p.title && html`<span class="ttl">${p.title}</span>`}
    <span class="nm">${p.name}</span>
    ${p.patron && html`<${Wing} />`}
    <span class="rt num">${p.rating}</span>
  </span>`;
}

export const Pair = ({ game }) =>
  html`<div class="pair"><${Player} p=${game.white} side="w" /><${Player} p=${game.black} side="b" /></div>`;

export const GameLink = ({ id, title }) =>
  html`<a class="mono" href=${`https://lichess.org/${id}`} target="_blank" rel="noopener" title=${title}>${id}</a>`;

// Average of both ratings; games against the computer aren't ranked.
export function GameRating({ game }) {
  if (game.white.ai || game.black.ai) {
    return html`<span class="rt mono" title="Game against the computer, not ranked">—</span>`;
  }
  return html`<span class="grating num">${fmt(Math.round((game.white.rating + game.black.rating) / 2))}</span>`;
}

function LookupStatus() {
  const l = lookup.value;
  const t = now.value;
  if (l.state === "ratelimited") {
    return html`<div class="lookup" style="--tone: var(--warn)">
      <span class="dot"></span>
      <span>
        <b>Rate-limited.</b> lichess answered 429, so the next request waits a full minute: in${" "}
        <span class="num">${Math.max(0, Math.ceil((l.retryAt - t) / 1000))} s</span>.${" "}
        <span class="num">${l.queued}</span> games waiting.
      </span>
    </div>`;
  }
  return html`<div class="lookup" style="--tone: var(--ok)">
    <span class="dot"></span>
    ${l.lastAt
      ? html`<span>
          <b>Lookup working.</b> Last request ${hms(l.lastAt)} (${l.lastCount} games) ·${" "}
          <span class="num">${l.queued}</span> waiting ·${" "}
          <span class="num">${fmt(l.missingAll)}</span> not found on lichess, all-time
        </span>`
      : html`<span><b>Lookup on.</b> The first request goes out 60 s after the first batch, or as soon as 300 games are waiting.</span>`}
  </div>`;
}

const TOP_RANGES = [["24h", "24 h"], ["7d", "7 d"], ["all", "All time"]];
const TOP_LIMIT = { "24h": 864e5, "7d": 7 * 864e5, all: Infinity };

export function TopGames() {
  const range = useSignal("all");
  const t = now.value;
  const rows = topGames.value
    .filter((g) => t - g.at < TOP_LIMIT[range.value])
    .sort((a, b) => b.rating - a.rating)
    .slice(0, 8);
  return html`<${Panel}
    title="Top games"
    actions=${html`<${Seg} label="Period" options=${TOP_RANGES} value=${range.value} onChange=${(v) => (range.value = v)} />`}
  >
    <${LookupStatus} />
    ${topGames.value.length
      ? html`<div class="scroll">
          <table class="cards">
            <thead>
              <tr><th class="r">#</th><th>Players</th><th class="r">Rating</th><th>Speed</th><th>Analysed</th><th>Game</th></tr>
            </thead>
            <tbody>
              ${rows.map(
                (g, i) => html`<tr key=${g.id} class=${i === 0 ? "first" : ""}>
                  <td class="r top rank o1">#${i + 1}</td>
                  <td class="wide o3"><${Pair} game=${g} /></td>
                  <td class="r o2"><span class="grating num">${fmt(g.rating)}</span></td>
                  <td class="o4" data-label="Speed">
                    <span class="chip">${g.speed}</span>${g.variant && html` <span class="chip">${g.variant}</span>`}
                  </td>
                  <td class="num mono o5" data-label="Analysed">${when(g.at, t)}</td>
                  <td class="o6" data-label="Game"><${GameLink} id=${g.id} title=${g.example && "Example ID, not a real game"} /></td>
                </tr>`,
              )}
            </tbody>
          </table>
        </div>`
      : html`<${Empty} title="No ranked games yet.">Games appear here once lichess has sent back their players.<//>`}
    <div class="note">
      Game rating is the average of both players' ratings when the game was played. Games against the computer aren't
      ranked.
    </div>
  <//>`;
}

function PStat({ label, all, today, total }) {
  return html`<div class="pstat">
    <div class="label">${label}</div>
    <div class="value num">${fmt(all)}<small>all-time</small></div>
    <div class="sub num">${total ? `${today} today · ${share(all, total)} of games` : "no games looked up yet"}</div>
  </div>`;
}

export function PlayerStats() {
  const p = players.value;
  const max = Math.max(1, ...p.byTitle.map(([, n]) => n));
  return html`<${Panel} title="Titled players and supporters" hint="games with at least one">
    <div class="pstats">
      <${PStat} label=${html`<span class="ttl">GM</span>Titled player`} all=${p.all.titled} today=${p.today.titled} total=${p.all.looked} />
      <${PStat} label=${html`<${Wing} label=${false} />Lichess supporter`} all=${p.all.patron} today=${p.today.patron} total=${p.all.looked} />
    </div>
    ${p.byTitle.length > 0 &&
    html`<div class="vbars">
      ${p.byTitle.map(
        ([title, n]) => html`<div class="vbar nopct">
          <span class="ttl">${title}</span>
          <div class="track"><div class="fill title" style=${`width: ${((n / max) * 100).toFixed(1)}%`}></div></div>
          <span class="n num">${fmt(n)}</span>
        </div>`,
      )}
    </div>`}
    <div class="note">
      ${p.all.looked
        ? `By title, all-time. A game between two titled players counts for both titles. Bot games are counted separately (${fmt(p.all.bots)} all-time). Shares are of the ${fmt(p.all.looked)} games found on lichess.`
        : "Counts start with the first looked-up game. A game between two titled players counts for both titles."}
    </div>
  <//>`;
}
