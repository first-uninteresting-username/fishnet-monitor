// shadcn's Button with Material 3 common-button styling: full-round shape,
// label-large text, a state layer, and M3's variants (filled, tonal, outlined,
// text, elevated) under shadcn's variant names.
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn, stateLayer } from "../../lib/utils.js";

export const buttonVariants = cva(
  cn(
    "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-full text-sm font-medium tracking-[0.00714em] transition-shadow duration-200 ease-standard disabled:pointer-events-none disabled:opacity-38 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-[18px] shrink-0 [&_svg]:shrink-0 outline-none focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-ring",
    stateLayer,
  ),
  {
    variants: {
      variant: {
        // M3 filled
        default: "bg-primary text-primary-foreground hover:shadow-elevation-1",
        destructive: "bg-destructive text-destructive-foreground hover:shadow-elevation-1",
        // M3 outlined
        outline: "border border-input text-primary",
        // M3 filled tonal
        secondary: "bg-secondary text-secondary-foreground hover:shadow-elevation-1",
        // M3 text
        ghost: "text-primary",
        link: "text-primary underline-offset-4 hover:underline before:hidden",
        // M3 elevated
        elevated: "bg-card text-primary shadow-elevation-1 hover:shadow-elevation-2",
      },
      size: {
        default: "h-10 px-6 has-[>svg]:pl-4",
        sm: "h-8 gap-1.5 px-4 has-[>svg]:pl-3",
        lg: "h-14 px-7 text-base has-[>svg]:pl-5",
        icon: "size-10",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export const Button = ({ class: cls, variant, size, type = "button", children, ...props }) =>
  html`<button data-slot="button" type=${type} class=${cn(buttonVariants({ variant, size }), cls)} ...${props}>${children}</button>`;
