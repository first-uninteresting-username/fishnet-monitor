// shadcn's Toggle + ToggleGroup (type "single"). Upstream builds on Radix;
// here it is a group of buttons with aria-pressed.
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

export const toggleVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-md text-sm font-medium hover:bg-muted hover:text-muted-foreground disabled:pointer-events-none disabled:opacity-50 data-[state=on]:bg-accent data-[state=on]:text-accent-foreground data-[state=on]:font-semibold [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0 focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] outline-none transition-[color,box-shadow] whitespace-nowrap",
  {
    variants: {
      variant: {
        default: "bg-transparent",
        outline: "border border-input bg-transparent shadow-xs hover:bg-accent hover:text-accent-foreground",
      },
      size: {
        default: "h-9 px-2 min-w-9",
        sm: "h-8 px-1.5 min-w-8",
        lg: "h-10 px-2.5 min-w-10",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

// `options`: [[value, label]].
export function ToggleGroup({ options, value, onChange, label, variant = "outline", size = "sm", class: cls, item }) {
  return html`<div
    data-slot="toggle-group"
    data-variant=${variant}
    role="group"
    aria-label=${label}
    class=${cn("group/toggle-group flex w-fit items-center rounded-md data-[variant=outline]:shadow-xs", cls)}
  >
    ${options.map(([v, text]) => {
      const on = v === value;
      return html`<button
        type="button"
        data-slot="toggle-group-item"
        data-variant=${variant}
        data-state=${on ? "on" : "off"}
        aria-pressed=${String(on)}
        class=${cn(
          toggleVariants({ variant, size }),
          "min-w-0 flex-1 shrink-0 rounded-none shadow-none first:rounded-l-md last:rounded-r-md focus:z-10 focus-visible:z-10 data-[variant=outline]:border-l-0 data-[variant=outline]:first:border-l",
          item,
        )}
        onClick=${() => onChange(v)}
      >${text}</button>`;
    })}
  </div>`;
}
