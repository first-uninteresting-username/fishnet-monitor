// Everything that needs the game lookup: player cells, top games, the
// titled/supporter panel and the lookup status line.
import { useSignal } from "@preact/signals";
import { html } from "../html.js";
import { lookup, now, players, topGames } from "../store.js";
import { fmt, hms, share, when } from "../format.js";
import { cn } from "../lib/utils.js";
import { Bar, cardsTable, Empty, headClass, Note, Panel, Seg, Wing } from "./common.js";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "./ui/table.js";

// Player title (GM, IM, …), copper like lichess's title tag.
export const TITLE = "text-title font-mono text-[11.5px] font-semibold";

const Side = ({ side }) =>
  html`<i class=${cn("border-muted-foreground size-2 shrink-0 rounded-[2px] border", side === "w" ? "bg-side-w" : "bg-side-b")}></i>`;

// Speed, variant and engine: plain small mono text, no box.
export const Chip = ({ class: cls, children }) =>
  html`<span class=${cn("text-muted-foreground font-mono text-xs", cls)}>${children}</span>`;

const PENDING = "text-muted-foreground text-[12.5px] italic";

export function Player({ p, side }) {
  if (p.ai) {
    return html`<span class="inline-flex items-center gap-1.5 whitespace-nowrap"><${Side} side=${side} /><span class="text-muted-foreground italic">Stockfish level ${p.ai}</span></span>`;
  }
  return html`<span class="inline-flex items-center gap-1.5 whitespace-nowrap">
    <${Side} side=${side} />
    ${p.title && html`<span class=${TITLE}>${p.title}</span>`}
    <span>${p.name}</span>
    ${p.patron && html`<${Wing} />`}
    <span class="text-muted-foreground font-mono text-xs tabular-nums">${p.rating}</span>
  </span>`;
}

export const Pair = ({ game }) =>
  html`<div class="grid gap-0.5"><${Player} p=${game.white} side="w" /><${Player} p=${game.black} side="b" /></div>`;

export const GameLink = ({ id, title }) =>
  html`<a class="text-primary font-mono hover:underline" href=${`https://lichess.org/${id}`} target="_blank" rel="noopener" title=${title}>${id}</a>`;

// Average of both ratings; games against the computer aren't ranked.
export function GameRating({ game }) {
  if (game.white.ai || game.black.ai) {
    return html`<span class="text-muted-foreground font-mono text-xs" title="Game against the computer, not ranked">—</span>`;
  }
  return html`<span class=${RATING}>${fmt(Math.round((game.white.rating + game.black.rating) / 2))}</span>`;
}

const RATING = "text-[15px] font-semibold tabular-nums";

const Lookup = ({ tone, children }) =>
  html`<p class="text-muted-foreground text-xs [&_b]:font-semibold [&_b]:text-(--tone)" style=${`--tone: var(--${tone})`}>${children}</p>`;

function LookupStatus() {
  const l = lookup.value;
  const t = now.value;
  if (l.state === "ratelimited") {
    return html`<${Lookup} tone="warn">
      <span>
        <b>Rate-limited.</b> lichess answered 429, so the next request waits a full minute: in${" "}
        <span class="tabular-nums">${Math.max(0, Math.ceil((l.retryAt - t) / 1000))} s</span>.${" "}
        <span class="tabular-nums">${l.queued}</span> games waiting.
      </span>
    <//>`;
  }
  return html`<${Lookup} tone="ok">
    ${l.lastAt
      ? html`<span>
          <b>Lookup working.</b> Last request ${hms(l.lastAt)} (${l.lastCount} games) ·${" "}
          <span class="tabular-nums">${l.queued}</span> waiting ·${" "}
          <span class="tabular-nums">${fmt(l.missingAll)}</span> not found on lichess, all-time
        </span>`
      : html`<span><b>Lookup on.</b> The first request goes out 60 s after the first batch, or as soon as 300 games are waiting.</span>`}
  <//>`;
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
      ? html`<${Table} class=${cn("text-[13px]", cardsTable)}>
          <${TableHeader}>
            <${TableRow} class="hover:bg-transparent">
              <${TableHead} class=${cn(headClass, "text-right")}>#<//>
              <${TableHead} class=${headClass}>Players<//>
              <${TableHead} class=${cn(headClass, "text-right")}>Rating<//>
              <${TableHead} class=${headClass}>Speed<//>
              <${TableHead} class=${headClass}>Analysed<//>
              <${TableHead} class=${headClass}>Game<//>
            <//>
          <//>
          <${TableBody}>
            ${rows.map(
              (g, i) => html`<${TableRow} key=${g.id}>
                <${TableCell} class=${cn("text-muted-foreground text-right align-top font-mono text-xs max-sm:order-1 max-sm:text-left", i === 0 && "text-title font-semibold")}>#${i + 1}<//>
                <${TableCell} class="max-sm:order-3 max-sm:col-span-full"><${Pair} game=${g} /><//>
                <${TableCell} class="text-right max-sm:order-2"><span class=${RATING}>${fmt(g.rating)}</span><//>
                <${TableCell} class="max-sm:order-4" data-label="Speed">
                  <${Chip}>${g.speed}${g.variant && ` · ${g.variant}`}<//>
                <//>
                <${TableCell} class="font-mono tabular-nums max-sm:order-5 max-sm:text-right" data-label="Analysed">${when(g.at, t)}<//>
                <${TableCell} class="max-sm:order-6" data-label="Game"><${GameLink} id=${g.id} title=${g.example && "Example ID, not a real game"} /><//>
              <//>`,
            )}
          <//>
        <//>`
      : html`<${Empty} title="No ranked games yet.">Games appear here once lichess has sent back their players.<//>`}
    <${Note}>Rating is the average of both players' ratings at the time. Games against the computer aren't ranked.<//>
  <//>`;
}

function PStat({ label, all, today, total }) {
  return html`<div class="grid gap-0.5">
    <div class="text-muted-foreground flex items-center gap-1.5 text-xs">${label}</div>
    <div class="text-2xl font-semibold tabular-nums">
      ${fmt(all)}<small class="text-muted-foreground ml-1 text-xs font-medium">all-time</small>
    </div>
    <div class="text-muted-foreground text-xs tabular-nums">${total ? `${today} today · ${share(all, total)} of games` : "no games looked up yet"}</div>
  </div>`;
}

export function PlayerStats() {
  const p = players.value;
  const max = Math.max(1, ...p.byTitle.map(([, n]) => n));
  return html`<${Panel} title="Titled players and supporters" hint="games with at least one">
    <div class="grid grid-cols-2 gap-x-6 gap-y-3 max-[440px]:grid-cols-1">
      <${PStat} label=${html`<span class=${TITLE}>GM</span>Titled player`} all=${p.all.titled} today=${p.today.titled} total=${p.all.looked} />
      <${PStat} label=${html`<${Wing} label=${false} />Lichess supporter`} all=${p.all.patron} today=${p.today.patron} total=${p.all.looked} />
    </div>
    ${p.byTitle.length > 0 &&
    html`<div class="grid gap-1.5">
      ${p.byTitle.map(
        ([title, n]) => html`<${Bar} label=${html`<span class=${cn(TITLE, "text-[12.5px]")}>${title}</span>`} pct=${(n / max) * 100} fill="bg-title" n=${fmt(n)} />`,
      )}
    </div>`}
    <${Note}>
      ${p.all.looked
        ? `By title, all-time. A game between two titled players counts for both titles. Bot games are counted separately (${fmt(p.all.bots)} all-time). Shares are of the ${fmt(p.all.looked)} games found on lichess.`
        : "Counts start with the first looked-up game. A game between two titled players counts for both titles."}
    <//>
  <//>`;
}
