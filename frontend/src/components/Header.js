// Header with state badge and meta, plus the strips under it:
// stall countdown, fatal stop, stale data.
import { html } from "../html.js";
import { client, conn, now } from "../store.js";
import { badge, health, quiet, sinceActivity, stalled, stale, staleSince } from "../derived.js";
import { dur, hms, uptime } from "../format.js";
import { cn } from "../lib/utils.js";
import { Dot, Logo } from "./common.js";
import { ScheduleIcon, SyncProblemIcon, WarningIcon } from "./icons.js";
import { Alert, AlertDescription } from "./ui/alert.js";
import { Progress } from "./ui/progress.js";

const TONE_TEXT = { ok: "text-ok", warn: "text-warn", crit: "text-crit" };

function Meta({ label, children, waiting, class: cls, title }) {
  return html`<div class="grid gap-px">
    <dt class="text-muted-foreground text-xs font-medium tracking-[0.04em]">${label}</dt>
    <dd class=${cn("font-mono text-sm", waiting && "text-muted-foreground font-sans italic", cls)} title=${title}>${children}</dd>
  </div>`;
}

export function Header() {
  const c = client.value;
  const b = badge.value;
  const h = health.value;
  const t = now.value;
  // An M3 top app bar: logo, title-large heading, the state as an assist chip.
  return html`<header class="flex flex-wrap items-center gap-x-8 gap-y-4 py-2">
    <div class="mr-auto flex items-center gap-4">
      <${Logo} />
      <div class="grid gap-0.5">
        <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
          <h1 class="text-[22px] leading-7 font-normal">fishnet-monitor</h1>
          <span
            class="inline-flex h-8 items-center gap-2 rounded-sm border border-input px-3 text-sm font-medium tracking-[0.00714em] text-(--tone)"
            style=${`--tone: var(--${b.tone})`}
          >
            <${Dot} pulse=${b.pulse} /><span>${b.text}</span>
          </span>
        </div>
        <small class="text-muted-foreground font-mono text-xs">${c.host}${c.listen && ` · ${c.listen}`}</small>
      </div>
    </div>
    <dl class="flex flex-wrap gap-x-6 gap-y-2">
      <${Meta} label="Health" class=${TONE_TEXT[h.tone]}>${h.text}<//>
      <${Meta} label="Version" waiting=${!c.version} title=${c.version ? null : "Read from the summary line fishnet prints every 2 minutes"}>
        ${c.version ?? "waiting for summary"}
      <//>
      <${Meta} label="Cores">${c.cores ?? "—"}<//>
      <${Meta} label="Uptime" class="tabular-nums">${c.startedAt ? uptime((t - c.startedAt) / 1000) : "—"}<//>
      <${Meta} label="Last batch" class="tabular-nums" waiting=${!c.lastBatchAt}>
        ${c.lastBatchAt ? `${dur((t - c.lastBatchAt) / 1000)} ago` : "none yet"}
      <//>
      ${c.queue != null && html`<${Meta} label="Queue" class="tabular-nums" title="Needs FISHNET_VERBOSE=1">${c.queue} pending<//>`}
    </dl>
  </header>`;
}

// A shadcn Alert in the colour of `tone`: an M3 banner on a tonal container.
function Strip({ tone, icon: Icon, children }) {
  // No role="alert": the stall countdown changes every second.
  return html`<${Alert}
    role=${null}
    class="bg-[color-mix(in_srgb,var(--tone)_14%,var(--background))] [&_b]:text-(--tone) [&_code]:font-mono [&_code]:text-[12.5px] [&>svg]:text-(--tone)"
    style=${`--tone: var(--${tone})`}
  >
    <${Icon} />
    <${AlertDescription} class="text-foreground min-w-0 gap-1.5">${children}<//>
  <//>`;
}

export function StallStrip() {
  if (!quiet.value) return null;
  const s = sinceActivity.value;
  const limit = client.value.stallTimeout;
  const crit = stalled.value;
  return html`<${Strip} tone=${crit ? "crit" : "warn"} icon=${ScheduleIcon}>
    <div>
      <b>No batch and no idle line for ${dur(s)}.</b>${" "}
      ${crit
        ? html`<code>/healthz</code> now reports unhealthy, so Docker marks the container unhealthy.`
        : html`fishnet normally logs one or the other every few minutes. <code>/healthz</code> reports unhealthy in ${dur(limit - s)}.`}
    </div>
    <${Progress} class="max-w-[440px] bg-(--tone)/20" indicator="bg-(--tone) duration-400" value=${(100 * s) / limit} />
    <div class="text-muted-foreground flex w-full max-w-[440px] justify-between gap-3 text-[11px]">
      <span>last activity</span><span>unhealthy at ${Math.round(limit / 60)} min (<code>STALL_TIMEOUT</code>)</span>
    </div>
  <//>`;
}

export function FatalStrip() {
  const f = client.value.fatal;
  if (client.value.state !== "fatal" || !f) return null;
  return html`<${Strip} tone="crit" icon=${WarningIcon}>
    <div>
      <b>fishnet stopped its queue.</b> It asked for a client update or a reconfiguration, so no new work will arrive
      until the container is updated or restarted.
    </div>
    <div><code>${f.line}</code> · ${new Date(f.at).toLocaleDateString("en-GB", { day: "numeric", month: "short" })} ${hms(f.at)}</div>
  <//>`;
}

export function StaleStrip() {
  if (!stale.value) return null;
  const lost = conn.value.state === "lost";
  const since = hms(staleSince.value);
  return html`<${Strip} tone=${lost ? "warn" : "idle"} icon=${SyncProblemIcon}>
    <div>
      ${lost
        ? html`<b>Lost the connection to the monitor.</b> Reconnecting (attempt ${conn.value.attempt}). The figures below
            are from ${since} and may be out of date.`
        : html`<b>Showing data as of ${since}.</b> fishnet is not running, so nothing below changes until it starts again.`}
    </div>
  <//>`;
}
