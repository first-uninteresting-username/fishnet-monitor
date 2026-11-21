// shadcn's Badge as a small Material 3 chip: small shape (8 px), label text.
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const badgeVariants = cva(
  "inline-flex h-6 items-center justify-center rounded-sm border px-2 text-xs font-medium tracking-[0.04em] w-fit whitespace-nowrap shrink-0 [&>svg]:size-3.5 gap-1 [&>svg]:pointer-events-none focus-visible:outline-3 focus-visible:outline-ring overflow-hidden",
  {
    variants: {
      variant: {
        default: "border-transparent bg-primary-container text-primary-container-foreground",
        secondary: "border-transparent bg-secondary text-secondary-foreground",
        destructive: "border-transparent bg-destructive text-destructive-foreground",
        outline: "border-input text-foreground",
      },
    },
    defaultVariants: { variant: "default" },
  },
);

export const Badge = ({ class: cls, variant, children, ...props }) =>
  html`<span data-slot="badge" class=${cn(badgeVariants({ variant }), cls)} ...${props}>${children}</span>`;
