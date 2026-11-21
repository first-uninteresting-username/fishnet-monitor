// All dashboard state lives in signals. A data source (the live API, or the
// demo's mock) writes them; components read them, and only the components that
// read a signal re-render when it changes. Times are epoch milliseconds.
import { signal } from "@preact/signals";

// Wall clock for relative times and countdowns, ticked once a second.
export const now = signal(Date.now());

// The page's connection to /api/events. `since` is when data last arrived.
export const conn = signal({ state: "live", attempt: 0, since: null });

export const client = signal({
  host: "",
  listen: "",
  state: "running", // starting | running | idle | stopping | stopped | fatal
  version: null, // from the 2-minute summary line
  cores: null,
  queue: null, // pending batches; needs FISHNET_VERBOSE=1
  startedAt: null,
  lastBatchAt: null,
  lastActivityAt: null, // last batch or idle line; drives the stall warning
  idleUntil: null,
  stoppingBatches: 0,
  stoppedAt: null,
  fatal: null, // { line, at }
  stallTimeout: 900, // seconds, STALL_TIMEOUT
});

// { batches, positions, nodes: { today, all, spark }, knps: { last, nnue, hce, spark }, nnue: { mnps, spark } }
export const totals = signal(null);

// Positions per minute, per chart range:
// { "1h": { step, t: [s], v: [n], kind: ["busy" | "idle" | "stop" | "quiet"] }, ... }
export const positions = signal({});

// Batches by variant, last 7 days: [{ name, n, eval: "nnue" | "hce" }]
export const variants = signal([]);

// Grouped W:/E: lines: [{ lvl: "W" | "E" | "F", msg, n, first, last }]
export const problems = signal([]);

// Game lookup status. `retryAt` is set while lichess is rate-limiting us.
export const lookup = signal({
  enabled: true,
  state: "ok", // ok | ratelimited | error
  lastAt: null,
  lastCount: 0,
  queued: 0,
  missingAll: 0,
  retryAt: null,
});

// Games with a titled player / a supporter. `byTitle`: [[title, games]], all-time.
export const players = signal({
  today: { looked: 0, titled: 0, patron: 0 },
  all: { looked: 0, titled: 0, patron: 0, bots: 0 },
  byTitle: [],
});

// Highest-rated analysed games: [{ id, at, white, black, rating, speed, variant }]
// A player is { name, rating, title, patron } or { ai: level }.
export const topGames = signal([]);

// Latest batches, newest first: [{ key, at, id, variant, eval, knps, game, live }]
// `game` is { white, black } once the lookup has it, null while pending.
export const batches = signal([]);
