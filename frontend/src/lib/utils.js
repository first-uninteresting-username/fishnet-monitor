// shadcn/ui's class helper: join classes, later Tailwind classes win.
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export const cn = (...inputs) => twMerge(clsx(inputs));

// Material 3 state layer: an overlay in the content colour, 8 % on hover and
// 10 % on focus or press. It sits under the content and above the element's
// own background (`isolate` makes the negative z-index stay inside).
export const stateLayer =
  "relative isolate before:absolute before:inset-0 before:-z-10 before:rounded-[inherit] before:bg-current before:opacity-0 before:transition-opacity before:duration-200 before:ease-standard hover:before:opacity-8 focus-visible:before:opacity-10 active:before:opacity-10";
