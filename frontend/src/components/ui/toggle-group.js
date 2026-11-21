// shadcn's Toggle + ToggleGroup (type "single"), styled as a Material 3
// segmented button: outlined, full-round ends, the selected segment filled
// with secondary-container and marked with a check. Upstream builds on Radix;
// here it is a group of buttons with aria-pressed.
import { cva } from "class-variance-authority";
import { html } from "../../html.js";
import { cn, stateLayer } from "../../lib/utils.js";
import { CheckIcon } from "../icons.js";

export const toggleVariants = cva(
  cn(
    "inline-flex items-center justify-center gap-1.5 rounded-full text-sm font-medium tracking-[0.00714em] text-foreground disabled:pointer-events-none disabled:opacity-38 data-[state=on]:bg-secondary data-[state=on]:text-secondary-foreground [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-[18px] [&_svg]:shrink-0 outline-none focus-visible:outline-3 focus-visible:-outline-offset-3 focus-visible:outline-ring transition-colors duration-200 ease-standard whitespace-nowrap",
    stateLayer,
  ),
  {
    variants: {
      variant: {
        default: "bg-transparent",
        outline: "border border-input bg-transparent",
      },
      size: {
        default: "h-10 px-3 min-w-12",
        sm: "h-8 px-3 min-w-10",
        lg: "h-12 px-4 min-w-14",
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
    class=${cn("group/toggle-group flex w-fit items-center", cls)}
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
          "min-w-0 flex-1 shrink-0 rounded-none first:rounded-l-full last:rounded-r-full focus:z-10 focus-visible:z-10 data-[variant=outline]:border-l-0 data-[variant=outline]:first:border-l",
          item,
        )}
        onClick=${() => onChange(v)}
      >${on && html`<${CheckIcon} class="-ml-0.5 size-4" />`}${text}</button>`;
    })}
  </div>`;
}
