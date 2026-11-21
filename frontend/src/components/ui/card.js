// shadcn's Card as a Material 3 card: medium shape (12 px), 16 px padding.
// `variant`: "elevated" (default; surface-container-low, elevation 1),
// "filled" (surface-container-highest) or "outlined" (surface, outline-variant).
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const cardVariants = cva("text-card-foreground flex flex-col gap-4 rounded-md py-4", {
  variants: {
    variant: {
      elevated: "bg-card shadow-elevation-1",
      filled: "bg-muted",
      outlined: "bg-background border",
    },
  },
  defaultVariants: { variant: "elevated" },
});

export const Card = ({ class: cls, variant, children, ...props }) =>
  html`<div data-slot="card" class=${cn(cardVariants({ variant }), cls)} ...${props}>${children}</div>`;

export const CardHeader = ({ class: cls, children, ...props }) =>
  html`<div
    data-slot="card-header"
    class=${cn("@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-1 px-4 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-4", cls)}
    ...${props}
  >${children}</div>`;

// M3 title-medium.
export const CardTitle = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-title" class=${cn("text-base leading-6 font-medium tracking-[0.009em]", cls)} ...${props}>${children}</div>`;

export const CardDescription = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-description" class=${cn("text-muted-foreground text-sm", cls)} ...${props}>${children}</div>`;

export const CardAction = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-action" class=${cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", cls)} ...${props}>${children}</div>`;

export const CardContent = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-content" class=${cn("px-4", cls)} ...${props}>${children}</div>`;

export const CardFooter = ({ class: cls, children, ...props }) =>
  html`<div data-slot="card-footer" class=${cn("flex items-center px-4 [.border-t]:pt-4", cls)} ...${props}>${children}</div>`;
