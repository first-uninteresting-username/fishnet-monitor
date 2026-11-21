import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const Card = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card" class=${cn("bg-card text-card-foreground flex flex-col gap-6 rounded-xl border py-6 shadow-sm", cls)} ...${props}>${children}</div>`;

export const CardHeader = ({ class: cls, children, ...props }) =>
  html`<div
    data-slot="card-header"
    class=${cn("@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6", cls)}
    ...${props}
  >${children}</div>`;

export const CardTitle = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-title" class=${cn("leading-none font-semibold", cls)} ...${props}>${children}</div>`;

export const CardDescription = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-description" class=${cn("text-muted-foreground text-sm", cls)} ...${props}>${children}</div>`;

export const CardAction = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-action" class=${cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", cls)} ...${props}>${children}</div>`;

export const CardContent = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-content" class=${cn("px-6", cls)} ...${props}>${children}</div>`;

export const CardFooter = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-footer" class=${cn("flex items-center px-6 [.border-t]:pt-6", cls)} ...${props}>${children}</div>`;
