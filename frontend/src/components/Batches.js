import { html } from "../html.js";
import { batches, client, conn, lookup } from "../store.js";
import { quiet, stale } from "../derived.js";
import { fmt, hms } from "../format.js";
import { cn } from "../lib/utils.js";
import { Dot, Empty, Panel, cardsTable, headClass } from "./common.js";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "./ui/table.js";
import { Chip, GameLink, GameRating, Pair } from "./Players.js";

const ENGINE = { nnue: "text-primary bg-primary/10", hce: "text-hce bg-hce/15" };

function Live() {
  const lost = conn.value.state === "lost";
  const on = !stale.value && client.value.state === "running" && !quiet.value && batches.value.length > 0;
  const tone = lost ? "warn" : on ? "ok" : "idle";
  return html`<span class="text-muted-foreground inline-flex items-center gap-1.5 text-xs" style=${`--tone: var(--${tone})`}>
    <${Dot} pulse=${on} class="size-[7px]" /><span>${lost ? "reconnecting…" : on ? "live" : "no new batches"}</span>
  </span>`;
}

function Row({ b, withPlayers, ratelimited }) {
  const pending = html`<span class="text-muted-foreground text-[12.5px] italic">${ratelimited ? "Waiting: lichess is rate-limiting lookups" : "Looking up players…"}</span>`;
  return html`<${TableRow} class=${b.live ? "motion-safe:animate-flash" : ""}>
    <${TableCell} class="align-top font-mono tabular-nums max-sm:order-1">${hms(b.at)}<//>
    ${withPlayers &&
    html`<${TableCell} class="max-sm:order-3 max-sm:col-span-full">${b.game ? html`<${Pair} game=${b.game} />` : pending}<//>
      <${TableCell} class="text-right max-sm:order-4 max-sm:text-left" data-label="Rating">
        ${b.game ? html`<${GameRating} game=${b.game} />` : html`<span class="text-muted-foreground italic">…</span>`}
      <//>`}
    <${TableCell} class="max-sm:order-5 max-sm:text-right" data-label="Variant">${b.variant} <${Chip} class=${ENGINE[b.eval]}>${b.eval}<//><//>
    <${TableCell} class="text-right font-mono tabular-nums max-sm:order-6 max-sm:text-left" data-label="knps/core">${fmt(b.knps)}<//>
    <${TableCell} class="max-sm:order-2 max-sm:text-right"><${GameLink} id=${b.id} title=${b.example && "Example ID, not a real game"} /><//>
  <//>`;
}

export function Batches() {
  const list = batches.value;
  const l = lookup.value;
  return html`<${Panel} title="Recent batches" actions=${html`<${Live} />`}>
    ${list.length
      ? html`<${Table} class=${cn("text-[13px]", cardsTable)}>
          <${TableHeader}>
            <${TableRow} class="hover:bg-transparent">
              <${TableHead} class=${headClass}>Time<//>
              ${l.enabled && html`<${TableHead} class=${headClass}>Players<//><${TableHead} class=${cn(headClass, "text-right")}>Rating<//>`}
              <${TableHead} class=${headClass}>Variant<//>
              <${TableHead} class=${cn(headClass, "text-right")}>knps/core<//>
              <${TableHead} class=${headClass}>Game<//>
            <//>
          <//>
          <${TableBody}>
            ${list.map((b) => html`<${Row} key=${b.key} b=${b} withPlayers=${l.enabled} ratelimited=${l.state === "ratelimited"} />`)}
          <//>
        <//>`
      : html`<${Empty} title="Waiting for the first batch.">
          fishnet asks lichess for work as soon as it starts. When lichess has nothing to analyse, the client goes idle and
          this stays empty.
        <//>`}
  <//>`;
}
