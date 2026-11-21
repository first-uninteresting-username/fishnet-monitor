// Header with state badge and meta, plus the strips under it:
// stall countdown, fatal stop, stale data.
import { html } from "../html.js";
import { client, conn, now } from "../store.js";
import { badge, health, quiet, sinceActivity, stalled, stale, staleSince } from "../derived.js";
import { dur, hms, uptime } from "../format.js";
import { AlertIcon, ClockIcon, Logo, ReconnectIcon } from "./ui.js";

function Meta({ label, children, waiting, class: cls = "", title }) {
  return html`<div>
    <span>${label}</span>
    <b class=${`${cls}${waiting ? " waiting" : ""}`} title=${title}>${children}</b>
  </div>`;
}

export function Header() {
  const c = client.value;
  const b = badge.value;
  const h = health.value;
  const t = now.value;
  return html`<header class="top">
    <div class="brand">
      <${Logo} />
      <div>
        <h1>fishnet-monitor</h1>
        <small>${c.host}${c.listen && ` · ${c.listen}`}</small>
      </div>
    </div>
    <span class=${`badge${b.pulse ? " pulse" : ""}`} style=${`--state: var(--${b.tone})`}>
      <span class="dot"></span><span>${b.text}</span>
    </span>
    <div class="meta">
      <${Meta} label="Health" class=${`health ${h.tone}`}>${h.text}<//>
      <${Meta} label="Version" waiting=${!c.version} title=${c.version ? null : "Read from the summary line fishnet prints every 2 minutes"}>
        ${c.version ?? "waiting for summary"}
      <//>
      <${Meta} label="Cores">${c.cores ?? "—"}<//>
      <${Meta} label="Uptime" class="num">${c.startedAt ? uptime((t - c.startedAt) / 1000) : "—"}<//>
      <${Meta} label="Last batch" class="num" waiting=${!c.lastBatchAt}>
        ${c.lastBatchAt ? `${dur((t - c.lastBatchAt) / 1000)} ago` : "none yet"}
      <//>
      ${c.queue != null && html`<${Meta} label="Queue" class="num" title="Needs FISHNET_VERBOSE=1">${c.queue} pending<//>`}
    </div>
  </header>`;
}

function Strip({ tone, icon: Icon, children }) {
  return html`<div class="strip" style=${`--tone: var(--${tone})`}>
    <${Icon} />
    <div class="body">${children}</div>
  </div>`;
}

export function StallStrip() {
  if (!quiet.value) return null;
  const s = sinceActivity.value;
  const limit = client.value.stallTimeout;
  const crit = stalled.value;
  return html`<${Strip} tone=${crit ? "crit" : "warn"} icon=${ClockIcon}>
    <div>
      <b>No batch and no idle line for ${dur(s)}.</b>${" "}
      ${crit
        ? html`<code>/healthz</code> now reports unhealthy, so Docker marks the container unhealthy.`
        : html`fishnet normally logs one or the other every few minutes. <code>/healthz</code> reports unhealthy in ${dur(limit - s)}.`}
    </div>
    <div class="meter"><i style=${`width: ${Math.min(100, (100 * s) / limit)}%`}></i></div>
    <div class="meter-labels">
      <span>last activity</span><span>unhealthy at ${Math.round(limit / 60)} min (<code>STALL_TIMEOUT</code>)</span>
    </div>
  <//>`;
}

export function FatalStrip() {
  const f = client.value.fatal;
  if (client.value.state !== "fatal" || !f) return null;
  return html`<${Strip} tone="crit" icon=${AlertIcon}>
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
  return html`<${Strip} tone=${lost ? "warn" : "idle"} icon=${ReconnectIcon}>
    <div>
      ${lost
        ? html`<b>Lost the connection to the monitor.</b> Reconnecting (attempt ${conn.value.attempt}). The figures below
            are from ${since} and may be out of date.`
        : html`<b>Showing data as of ${since}.</b> fishnet is not running, so nothing below changes until it starts again.`}
    </div>
  <//>`;
}
