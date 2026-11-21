import { html } from "../../html.js";
import { cn } from "../../lib/utils.js";

// `value` is 0–100. `indicator` adds classes to the filled part.
export const Progress = ({ class: cls, indicator, value = 0, ...props }) =>
  html`<div
    data-slot="progress"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow=${Math.round(value)}
    class=${cn("bg-primary/20 relative h-2 w-full overflow-hidden rounded-full", cls)}
    ...${props}
  >
    <div
      data-slot="progress-indicator"
      class=${cn("bg-primary h-full w-full flex-1 transition-all", indicator)}
      style=${`transform: translateX(-${100 - Math.min(100, Math.max(0, value))}%)`}
    ></div>
  </div>`;
