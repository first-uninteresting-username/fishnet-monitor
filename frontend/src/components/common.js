// Building blocks shared by the panels, made from the shadcn components in ui/.
import { html } from "../html.js";
import { cn } from "../lib/utils.js";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card.js";
import { Empty as EmptyRoot, EmptyDescription, EmptyTitle } from "./ui/empty.js";
import { ToggleGroup } from "./ui/toggle-group.js";

// Segmented text buttons: a shadcn ToggleGroup sized for words, not icons.
// `options`: [[value, label]].
export const Seg = ({ item, ...props }) => html`<${ToggleGroup} item=${cn("h-7 flex-none px-2.5 text-xs", item)} ...${props} />`;

export function Empty({ title, children }) {
  return html`<${EmptyRoot} class="gap-1 rounded-lg border border-dashed px-4 py-7 md:p-7">
    ${title && html`<${EmptyTitle} class="text-sm font-medium">${title}<//>`}
    <${EmptyDescription} class="text-[13px]">${children}<//>
  <//>`;
}

export function Panel({ title, hint, actions, class: cls, children }) {
  return html`<${Card} class=${cn("min-w-0 gap-3 py-4", cls)}>
    <${CardHeader} class="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 px-4">
      <${CardTitle} class="mr-auto text-[15px]">${title}<//>
      ${hint != null && hint !== "" && html`<${CardDescription} class="text-xs tabular-nums">${hint}<//>`}
      ${actions && html`<div class="flex flex-wrap items-center gap-2">${actions}</div>`}
    <//>
    <${CardContent} class="grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-2.5 px-4">${children}<//>
  <//>`;
}

// Small print under a panel.
export const Note = ({ children }) => html`<div class="text-muted-foreground text-xs">${children}</div>`;

// Colour swatches under a chart or bar list. `items`: [[class, label]].
export const Legend = ({ items }) =>
  html`<div class="text-muted-foreground flex flex-wrap gap-x-3.5 gap-y-1.5 text-xs">
    ${items.map(
      ([swatch, label]) => html`<span class="inline-flex items-center gap-1.5"><i class=${cn("size-2.5 rounded-xs", swatch)}></i>${label}</span>`,
    )}
  </div>`;

// A dot in the colour of `--tone` (set by the caller). `pulse` animates it.
export const Dot = ({ pulse, class: cls }) =>
  html`<span class=${cn("size-2 shrink-0 rounded-full bg-(--tone)", pulse && "motion-safe:animate-pulse", cls)}></span>`;

export const Logo = () => html`<svg class="text-primary size-[30px] shrink-0" viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true">
  <path d="M4 7 Q16 3 28 7 L25 26 Q16 29 7 26 Z" fill="none" />
  <path d="M6 13 Q16 10 26.5 13 M7 19.5 Q16 17 25.5 19.5 M10 5.5 L12 27.5 M16 4.5 L16 28.5 M22 5.5 L20 27.5" />
</svg>`;

const icon = (paths) => () =>
  html`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">${paths}</svg>`;

export const ClockIcon = icon(html`<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />`);
export const AlertIcon = icon(html`<path d="M12 3 2 21h20L12 3Z" /><path d="M12 10v5M12 18v.5" />`);
export const ReconnectIcon = icon(
  html`<path d="M3 12a9 9 0 0 1 15.5-6.2M21 12a9 9 0 0 1-15.5 6.2" /><path d="M18 3v3h-3M6 21v-3h3" />`,
);

export function Wing({ label = true }) {
  return html`<svg class="text-primary h-2.5 w-3 shrink-0" viewBox="0 0 12 10" role=${label ? "img" : null} aria-label=${label ? "Lichess supporter" : null} aria-hidden=${label ? null : "true"}>
    ${label && html`<title>Lichess supporter</title>`}
    <path fill="currentColor" d="M.5 1.5c3.2-.4 5.4 1 6.3 3.6C7.6 3 9.1 1.8 11.5 1.5 11 5 9.3 8 6.4 9 3.4 8.3 1.2 5.6.5 1.5Z" />
  </svg>`;
}

// A horizontal bar list row (variants, titles): label, bar, count, optional share.
export function Bar({ label, pct, fill, n, share }) {
  return html`<div
    class=${cn(
      "grid items-center gap-2.5 text-[13px] max-[440px]:gap-2",
      share != null
        ? "grid-cols-[110px_minmax(0,1fr)_52px_44px] max-[440px]:grid-cols-[96px_minmax(0,1fr)_44px_40px]"
        : "grid-cols-[110px_minmax(0,1fr)_64px]",
    )}
  >
    <span>${label}</span>
    <div class="bg-muted h-3.5 overflow-hidden rounded-xs">
      <div class=${cn("h-full min-w-0.5 rounded-xs", fill)} style=${`width: ${pct.toFixed(2)}%`}></div>
    </div>
    <span class="text-muted-foreground text-right font-mono text-xs tabular-nums">${n}</span>
    ${share != null && html`<span class="text-right font-mono text-xs tabular-nums">${share}</span>`}
  </div>`;
}

// Classes that turn a shadcn Table into stacked cards on phones. Cells place
// themselves with `max-sm:order-*` / `max-sm:col-span-full`; cells with a
// `data-label` show it above the value.
export const cardsTable =
  "max-sm:block max-sm:[&_thead]:hidden max-sm:[&_tbody]:block max-sm:[&_tr]:grid max-sm:[&_tr]:grid-cols-[minmax(0,1fr)_auto] max-sm:[&_tr]:gap-x-3 max-sm:[&_tr]:gap-y-1.5 max-sm:[&_tr]:px-0.5 max-sm:[&_tr]:py-2.5 max-sm:[&_td]:block max-sm:[&_td]:min-w-0 max-sm:[&_td]:p-0 max-sm:[&_td]:whitespace-normal max-sm:[&_td[data-label]]:before:block max-sm:[&_td[data-label]]:before:font-sans max-sm:[&_td[data-label]]:before:text-[10.5px] max-sm:[&_td[data-label]]:before:font-normal max-sm:[&_td[data-label]]:before:tracking-wider max-sm:[&_td[data-label]]:before:text-muted-foreground max-sm:[&_td[data-label]]:before:uppercase max-sm:[&_td[data-label]]:before:content-[attr(data-label)]";

// Table head cell in the dashboard's small-caps style.
export const headClass = "text-muted-foreground h-auto px-2 py-1.5 text-[11px] font-medium tracking-wider uppercase";
