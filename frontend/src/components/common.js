// Building blocks shared by the panels, made from the shadcn components in ui/.
import { html } from "../html.js";
import { cn } from "../lib/utils.js";
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "./ui/card.js";
import { Empty as EmptyRoot, EmptyDescription, EmptyTitle } from "./ui/empty.js";
import { ToggleGroup } from "./ui/toggle-group.js";
import { PhishingIcon } from "./icons.js";

// Segmented text buttons: an M3 segmented button (shadcn ToggleGroup) at
// density -2 (32 px), sized for words, not icons. `options`: [[value, label]].
export const Seg = ({ item, ...props }) => html`<${ToggleGroup} item=${cn("h-8 min-w-0 flex-none px-3 text-[13px]", item)} ...${props} />`;

export function Empty({ title, children }) {
  return html`<${EmptyRoot} class="items-start gap-1 p-0 py-4 text-left md:p-0 md:py-4">
    ${title && html`<${EmptyTitle} class="text-sm font-medium">${title}<//>`}
    <${EmptyDescription} class="max-w-prose text-[13px]">${children}<//>
  <//>`;
}

// A section of the page: an M3 elevated card with a title-medium heading,
// an optional hint (supporting text) and actions on the right.
export function Panel({ title, hint, actions, class: cls, children }) {
  return html`<${Card} class=${cn("min-w-0", cls)}>
    <${CardHeader} class="flex min-h-8 flex-wrap items-center gap-x-3 gap-y-2">
      <${CardTitle} class="mr-auto"><h2>${title}</h2><//>
      ${hint != null && hint !== "" && html`<${CardDescription} class="text-xs tabular-nums">${hint}<//>`}
      ${actions && html`<${CardAction} class="flex flex-wrap items-center gap-2">${actions}<//>`}
    <//>
    <${CardContent} class="grid min-w-0 grid-cols-[minmax(0,1fr)] content-start gap-3">${children}<//>
  <//>`;
}

// Small print under a panel.
export const Note = ({ children }) => html`<div class="text-muted-foreground text-xs">${children}</div>`;

// Colour swatches under a chart or bar list. `items`: [[class, label]].
export const Legend = ({ items }) =>
  html`<div class="text-muted-foreground flex flex-wrap gap-x-3.5 gap-y-1.5 text-xs">
    ${items.map(
      ([swatch, label]) => html`<span class="inline-flex items-center gap-1.5"><i class=${cn("size-2.5 rounded-full", swatch)}></i>${label}</span>`,
    )}
  </div>`;

// A dot in the colour of `--tone` (set by the caller). `pulse` animates it.
export const Dot = ({ pulse, class: cls }) =>
  html`<span class=${cn("size-2 shrink-0 rounded-full bg-(--tone)", pulse && "motion-safe:animate-pulse", cls)}></span>`;

// The app icon: Material Symbols `phishing` (a fish hook) on primary-container,
// in M3's large shape. frontend/favicon.svg is the same mark.
export const Logo = () =>
  html`<span class="bg-primary-container text-primary-container-foreground grid size-10 shrink-0 place-items-center rounded-lg">
    <${PhishingIcon} />
  </span>`;

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
    <div class="bg-muted h-2 overflow-hidden rounded-full">
      <div class=${cn("h-full min-w-1 rounded-full", fill)} style=${`width: ${pct.toFixed(2)}%`}></div>
    </div>
    <span class="text-muted-foreground text-right font-mono text-xs tabular-nums">${n}</span>
    ${share != null && html`<span class="text-right font-mono text-xs tabular-nums">${share}</span>`}
  </div>`;
}

// Classes that turn a shadcn Table into stacked cards on phones. Cells place
// themselves with `max-sm:order-*` / `max-sm:col-span-full`; cells with a
// `data-label` show it above the value.
export const cardsTable =
  "max-sm:block max-sm:[&_thead]:hidden max-sm:[&_tbody]:block max-sm:[&_tr]:grid max-sm:[&_tr]:grid-cols-[minmax(0,1fr)_auto] max-sm:[&_tr]:gap-x-3 max-sm:[&_tr]:gap-y-1.5 max-sm:[&_tr]:px-0.5 max-sm:[&_tr]:py-2.5 max-sm:[&_td]:block max-sm:[&_td]:min-w-0 max-sm:[&_td]:p-0 max-sm:[&_td]:whitespace-normal max-sm:[&_td[data-label]]:before:block max-sm:[&_td[data-label]]:before:font-sans max-sm:[&_td[data-label]]:before:text-[11px] max-sm:[&_td[data-label]]:before:font-normal max-sm:[&_td[data-label]]:before:text-muted-foreground max-sm:[&_td[data-label]]:before:content-[attr(data-label)]";

// Table head cell: M3 label-medium, muted, normal case.
export const headClass = "text-muted-foreground h-auto px-2 py-2 text-xs font-medium tracking-[0.04em]";
