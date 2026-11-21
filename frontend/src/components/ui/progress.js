// shadcn's Progress as a Material 3 linear progress indicator: 4 px, rounded
// track in secondary-container, rounded active indicator.
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
    class=${cn("bg-secondary relative h-1 w-full overflow-hidden rounded-full", cls)}
    ...${props}
  >
    <div
      data-slot="progress-indicator"
      class=${cn("bg-primary h-full w-full flex-1 rounded-full transition-transform ease-standard", indicator)}
      style=${`transform: translateX(-${100 - Math.min(100, Math.max(0, value))}%)`}
    ></div>
  </div>`;
