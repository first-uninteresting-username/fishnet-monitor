// shadcn's Alert, shaped like a Material 3 banner: medium shape (12 px),
// tonal container, 20 px leading icon.
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

const alertVariants = cva(
  "relative w-full rounded-md px-4 py-3 text-sm grid has-[>svg]:grid-cols-[calc(var(--spacing)*5)_1fr] grid-cols-[0_1fr] has-[>svg]:gap-x-3 gap-y-0.5 items-start [&>svg]:size-5 [&>svg]:text-current",
  {
    variants: {
      variant: {
        default: "bg-surface-container-high text-card-foreground",
        destructive: "text-destructive bg-surface-container-high [&>svg]:text-current *:data-[slot=alert-description]:text-destructive/90",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export const Alert = ({ class: cls, variant, children, ...props }) =>
  html`<div data-slot="alert" role="alert" class=${cn(alertVariants({ variant }), cls)} ...${props}>${children}</div>`;

export const AlertTitle = ({ class: cls, children, ...props }) =>
  html`<div data-slot="alert-title" class=${cn("col-start-2 line-clamp-1 min-h-5 font-medium", cls)} ...${props}>${children}</div>`;

export const AlertDescription = ({ class: cls, children, ...props }) =>
  html`<div
    data-slot="alert-description"
    class=${cn("text-muted-foreground col-start-2 grid justify-items-start gap-1 text-sm [&_p]:leading-relaxed", cls)}
    ...${props}
  >${children}</div>`;
