// shadcn/ui's class helper: join classes, later Tailwind classes win.
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export const cn = (...inputs) => twMerge(clsx(inputs));
