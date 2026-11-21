import { html } from "../frontend/src/html.js";
import { Seg } from "../frontend/src/components/ui.js";
import { demo, setDemo } from "./mock.js";

const GROUPS = [
  ["state", "Client", [["running", "running"], ["stalling", "no batches"], ["idle", "idle"], ["stopping", "stopping"], ["stopped", "stopped"], ["fatal", "fatal"]]],
  ["conn", "Page connection", [["live", "live"], ["lost", "lost"]]],
  ["lookup", "Game lookup", [["ok", "working"], ["ratelimited", "rate-limited"], ["off", "off"]]],
  ["data", "History", [["full", "3 days"], ["empty", "first start"]]],
];

export function DemoControls() {
  const d = demo.value;
  return html`<div class="demo" role="group" aria-label="Demo controls">
    <strong>Demo</strong><span>Mock data, nothing is connected.</span>
    ${GROUPS.map(
      ([key, label, options]) => html`<span class="grp">
        ${label}
        <${Seg} label=${label} options=${options} value=${d[key]} onChange=${(v) => setDemo(key, v)} />
      </span>`,
    )}
  </div>`;
}
