// State the page works out from the raw signals: health, stall, stale data.
// Mirrors what /healthz decides on the server (PLAN.md, "Health").
import { computed } from "@preact/signals";
import { client, conn, now } from "./store.js";
import { dur } from "./format.js";

// After this long with no batch and no idle line, the stall strip appears.
export const QUIET_AFTER = 180;

export const sinceActivity = computed(() => {
  const at = client.value.lastActivityAt;
  return at == null ? null : Math.max(0, (now.value - at) / 1000);
});

export const stopped = computed(() => ["stopped", "fatal"].includes(client.value.state));

export const quiet = computed(
  () => client.value.state === "running" && sinceActivity.value != null && sinceActivity.value >= QUIET_AFTER,
);

export const stalled = computed(() => quiet.value && sinceActivity.value >= client.value.stallTimeout);

export const stale = computed(() => conn.value.state === "lost" || stopped.value);

export const staleSince = computed(() => (conn.value.state === "lost" ? conn.value.since : client.value.stoppedAt));

export const health = computed(() => {
  const c = client.value;
  if (stopped.value) return { tone: "crit", text: c.state === "fatal" ? "Unhealthy · fatal" : "Unhealthy · not running" };
  if (stalled.value) return { tone: "crit", text: "Unhealthy · stalled" };
  if (quiet.value) return { tone: "warn", text: `OK · unhealthy in ${dur(c.stallTimeout - sinceActivity.value)}` };
  return { tone: "ok", text: "OK" };
});

export const badge = computed(() => {
  const c = client.value;
  switch (c.state) {
    case "running":
      if (stalled.value) return { text: "Stalled", tone: "crit" };
      if (quiet.value) return { text: "Running · quiet", tone: "warn" };
      return { text: "Running", tone: "ok", pulse: true };
    case "idle":
      return { text: `Idle · back in ${dur((c.idleUntil - now.value) / 1000)}`, tone: "idle" };
    case "stopping":
      return { text: `Stopping · finishing ${c.stoppingBatches} batches`, tone: "warn", pulse: true };
    case "stopped":
      return { text: "Stopped", tone: "idle" };
    default:
      return { text: "Fatal", tone: "crit" };
  }
});
