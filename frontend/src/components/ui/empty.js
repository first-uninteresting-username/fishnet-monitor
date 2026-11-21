import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const Empty = ({ class: cls, children, ...props }) =>
  html`<div
    data-slot="empty"
    class=${cn("flex min-w-0 flex-1 flex-col items-center justify-center gap-6 rounded-lg border-dashed p-6 text-center text-balance md:p-12", cls)}
    ...${props}
  >${children}</div>`;

export const EmptyHeader = ({ class: cls, children, ...props }) =>
  html`<div data-slot="empty-header" class=${cn("flex max-w-sm flex-col items-center gap-2 text-center", cls)} ...${props}>${children}</div>`;

export const EmptyTitle = ({ class: cls, children, ...props }) =>
  html`<div data-slot="empty-title" class=${cn("text-lg font-medium tracking-tight", cls)} ...${props}>${children}</div>`;

export const EmptyDescription = ({ class: cls, children, ...props }) =>
  html`<div
    data-slot="empty-description"
    class=${cn("text-muted-foreground [&>a:hover]:text-primary text-sm/relaxed [&>a]:underline [&>a]:underline-offset-4", cls)}
    ...${props}
  >${children}</div>`;
