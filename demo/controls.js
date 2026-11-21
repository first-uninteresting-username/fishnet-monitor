import { html } from "../frontend/src/html.js";
import { Seg } from "../frontend/src/components/common.js";
import { demo, setDemo } from "./mock.js";

const GROUPS = [
  ["state", "Client", [["running", "running"], ["stalling", "no batches"], ["idle", "idle"], ["stopping", "stopping"], ["stopped", "stopped"], ["fatal", "fatal"]]],
  ["conn", "Page connection", [["live", "live"], ["lost", "lost"]]],
  ["lookup", "Game lookup", [["ok", "working"], ["ratelimited", "rate-limited"], ["off", "off"]]],
  ["data", "History", [["full", "3 days"], ["empty", "first start"]]],
];

export function DemoControls() {
  const d = demo.value;
  return html`<div
    class="text-muted-foreground bg-surface-container-high flex flex-wrap items-center gap-x-4.5 gap-y-2 rounded-lg px-4 py-3 text-xs"
    role="group"
    aria-label="Demo controls"
  >
    <strong class="text-foreground font-semibold">Demo</strong><span>Mock data, nothing is connected.</span>
    ${GROUPS.map(
      ([key, label, options]) => html`<span class="inline-flex flex-wrap items-center gap-1.5">
        ${label}
        <${Seg} class="flex-wrap" label=${label} options=${options} value=${d[key]} onChange=${(v) => setDemo(key, v)} />
      </span>`,
    )}
  </div>`;
}
