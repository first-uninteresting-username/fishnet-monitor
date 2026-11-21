// Mock data source for the dashboard demo. It writes the same signals the live
// API will write, and fakes the SSE stream: a clock tick every second and a new
// batch every ~6 s. Names and game IDs are made up.
import { batch, signal } from "@preact/signals";
import * as store from "../frontend/src/store.js";

// The demo's switches (see controls.js).
export const demo = signal({ state: "running", conn: "live", lookup: "ok", data: "full" });

const START = new Date(2026, 10, 21, 14, 7, 30).getTime();
const MIN = 60000;
const SEC = 1000;

// ---------- seeded random ----------
let seed = 7;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
const pick = (a) => a[Math.floor(rnd() * a.length)];

// ---------- players and games ----------
const ALPHABET = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const gameId = () => Array.from({ length: 8 }, () => pick(ALPHABET)).join("");
const TITLES = [["GM", 4], ["IM", 4], ["FM", 6], ["CM", 2], ["NM", 3], ["WGM", 1], ["WIM", 1], ["WFM", 1], ["LM", 2]];
const W1 = ["quiet", "Amber", "nord", "Pawn", "lazy", "Sharp", "kestrel", "Iron", "tempo", "Blue", "zugzwang", "Vela", "Otter", "fianchetto", "Mossy", "late"];
const W2 = ["Rook", "_knight", "Bishop", "owl", "Gambit", "fork", "Castle", "pin", "Lark", "Endgame", "Tiger", "file"];
const nick = () => pick(W1) + pick(W2) + (rnd() < 0.5 ? Math.floor(rnd() * 99) : "");
const pickTitle = () => {
  let r = rnd() * 24;
  for (const [t, w] of TITLES) if ((r -= w) < 0) return t;
  return "FM";
};
const player = (rating, titledP, patronP) => ({
  name: nick(),
  rating: Math.round(rating),
  title: rnd() < titledP ? pickTitle() : null,
  patron: rnd() < patronP,
});

function mockGame(base, spread, titledP, patronP, force = {}) {
  const r = base + (rnd() - 0.5) * spread;
  const white = player(r, titledP, patronP);
  const black = player(r + (rnd() - 0.5) * 240, titledP, patronP);
  if (force.title) white.title = force.title;
  if (force.patron) black.patron = true;
  if (force.ai) black.ai = force.ai;
  return { white, black };
}

// Top games: one pool of analysed games, so the 24 h / 7 d / all-time lists agree.
seed = 4242;
const SPEEDS = ["bullet", "blitz", "blitz", "blitz", "rapid", "rapid", "classical"];
const POOL = [];
for (const [n, lo, hi, base, spread] of [[15, 5, 1440, 2380, 420], [25, 1440, 7 * 1440, 2520, 480], [30, 7 * 1440, 120 * 1440, 2640, 560]]) {
  for (let i = 0; i < n; i++) {
    const g = mockGame(base, spread, 0.75, 0.25);
    g.at = START - (lo + rnd() * (hi - lo)) * MIN;
    g.speed = pick(SPEEDS);
    g.variant = rnd() < 0.12 ? pick(["chess960", "crazyhouse", "atomic"]) : null;
    g.id = gameId();
    g.example = true;
    g.rating = Math.round((g.white.rating + g.black.rating) / 2);
    POOL.push(g);
  }
}

// ---------- batches ----------
const VARIANTS = ["standard", "standard", "standard", "standard", "standard", "standard", "standard", "chess960", "crazyhouse", "atomic", "antichess"];
let batchKey = 0;
function mockBatch(at, live, force) {
  const variant = pick(VARIANTS);
  const ev = variant === "standard" || variant === "chess960" ? "nnue" : "hce";
  const knps = ev === "nnue" ? 1650 + Math.round(rnd() * 380) : 900 + Math.round(rnd() * 400);
  const id = gameId();
  const game = mockGame(1650, 1100, 0.025, 0.08, force);
  return { b: { key: ++batchKey, at, id, variant, eval: ev, knps, game: live ? null : game, live, example: true }, game };
}

seed = 1234;
const FORCED = { 2: { title: "IM" }, 4: { patron: true }, 6: { ai: 6 } };
const INITIAL_BATCHES = [];
for (let i = 0, t = START; i < 9; i++) {
  INITIAL_BATCHES.push(mockBatch(t, false, FORCED[i]).b);
  t -= (25 + rnd() * 40) * SEC;
}

// ---------- positions history ----------
const RANGES = { "1h": [60, 1, 11], "6h": [72, 5, 5], "24h": [288, 5, 7], "7d": [336, 30, 23] }; // buckets, minutes each, seed
const IDLE = [[1140, 1210], [790, 835], [360, 385], [210, 225], [540, 635]]; // minutes ago, within a day
const isIdle = (m) => IDLE.some(([a, b]) => m >= a && m < b);

function history(state, now, quietMin) {
  const out = {};
  for (const [range, [n, step, s]] of Object.entries(RANGES)) {
    seed = s;
    const d = { step: step * 60, t: [], v: [], kind: [] };
    for (let i = n - 1; i >= 0; i--) {
      const m = i * step;
      const daily = 0.75 + 0.25 * Math.sin((m / 1440) * Math.PI * 2 + 1.3);
      let v = (52 + rnd() * 28) * daily;
      let kind = "busy";
      if (range !== "7d" && isIdle(m % 1440)) v = 0;
      if (range === "7d" && rnd() < 0.12) v *= 0.3;
      if ((state === "stopped" || state === "fatal") && m < 5) [v, kind] = [0, "stop"];
      else if (state === "idle" && m < 3) v = 0;
      else if (state === "stalling" && m < quietMin) [v, kind] = [0, "quiet"];
      if (v === 0 && kind === "busy") kind = "idle";
      d.t.push(Math.round((now - m * MIN) / 1000));
      d.v.push(Math.round(v * 10) / 10);
      d.kind.push(kind);
    }
    out[range] = d;
  }
  return out;
}

function spark(base, jitter, s) {
  seed = s;
  return Array.from({ length: 40 }, (_, i) => base + (rnd() - 0.5) * jitter + Math.sin(i / 5) * jitter * 0.3);
}

const FULL_TOTALS = () => ({
  batches: { today: 412, all: 58931, spark: spark(30, 20, 101) },
  positions: { today: 18600, all: 2.71e6, spark: spark(60, 30, 202) },
  nodes: { today: 41.2e9, all: 6.03e12, spark: spark(50, 30, 303) },
  knps: { last: 1842, nnue: 1842, hce: 1104, spark: spark(1800, 300, 404) },
  nnue: { mnps: 14.7, spark: spark(14.5, 1.5, 505) },
});
const EMPTY_TOTALS = () => ({
  batches: { today: 0, all: 0 },
  positions: { today: 0, all: 0 },
  nodes: { today: 0, all: 0 },
  knps: { last: null },
  nnue: { mnps: null },
});

const FULL_PLAYERS = () => ({
  today: { looked: 401, titled: 9, patron: 34 },
  all: { looked: 58402, titled: 1284, patron: 4917, bots: 312 },
  byTitle: [["GM", 212], ["IM", 301], ["FM", 455], ["CM", 160], ["NM", 190], ["WGM", 18], ["WIM", 25], ["WFM", 41], ["LM", 77]],
});
const EMPTY_PLAYERS = () => ({ today: { looked: 0, titled: 0, patron: 0 }, all: { looked: 0, titled: 0, patron: 0, bots: 0 }, byTitle: [] });

const FATAL_LINE = "Client update or reconfiguration might be required. Stopping queue.";
const PROBLEMS = (now) => [
  { lvl: "W", msg: "Engine process exited unexpectedly (signal 9). Restarting engine.", n: 1, first: now - 139 * MIN, last: now - 139 * MIN },
  { lvl: "E", msg: "Network error: error sending request for url (https://fishnet.lichess.org/fishnet/acquire). Backing off 12s.", n: 3, first: now - 277 * MIN, last: now - 276 * MIN },
  { lvl: "W", msg: "Abandoning batch 8KpX2nQe after 3 failed submissions.", n: 2, first: now - 477 * MIN, last: now - 475 * MIN },
  { lvl: "W", msg: "Received 429 Too Many Requests. Backing off 30s.", n: 14, first: now - 969 * MIN, last: now - 962 * MIN },
];

// ---------- applying the switches ----------

// Seconds since the last batch or idle line, per client state.
const QUIET_FOR = { stalling: 9 * 60 + 40, stopped: 5 * 60 + 12, fatal: 5 * 60 + 12 };

function apply(prev) {
  const d = demo.value;
  const now = store.now.value;
  const empty = d.data === "empty";
  const state = d.state === "stalling" ? "running" : d.state;
  const changed = (k) => !prev || prev[k] !== d[k];
  batch(() => {
    if (changed("state") || changed("data")) {
      const since = QUIET_FOR[d.state] ?? 4;
      const startedAt = empty ? now - 41 * SEC : now - (3 * 86400 + 4 * 3600 + 12 * 60) * SEC;
      const stoppedAt = d.state === "stopped" || d.state === "fatal" ? now - since * SEC : null;
      store.client.value = {
        ...store.client.value,
        host: "nas-01",
        listen: ":9465",
        state,
        version: empty ? null : "v2.9.4",
        cores: 8,
        queue: 3,
        startedAt,
        lastActivityAt: empty && d.state !== "stalling" ? startedAt : now - since * SEC,
        lastBatchAt: empty && d.state !== "stalling" ? null : now - since * SEC,
        idleUntil: now + 52 * SEC,
        stoppingBatches: 2,
        stoppedAt,
        fatal: d.state === "fatal" ? { line: `E: ${FATAL_LINE}`, at: stoppedAt } : null,
      };
      store.positions.value = empty ? {} : history(d.state, now, since / 60);
      store.totals.value = empty ? EMPTY_TOTALS() : FULL_TOTALS();
      store.variants.value = empty ? [] : [
        { name: "standard", n: 4380, eval: "nnue" }, { name: "chess960", n: 241, eval: "nnue" },
        { name: "crazyhouse", n: 96, eval: "hce" }, { name: "antichess", n: 61, eval: "hce" },
        { name: "atomic", n: 54, eval: "hce" }, { name: "kingofthehill", n: 22, eval: "hce" },
        { name: "threecheck", n: 18, eval: "hce" }, { name: "horde", n: 9, eval: "hce" },
        { name: "racingkings", n: 6, eval: "hce" },
      ];
      const fatal = d.state === "fatal" ? [{ lvl: "F", msg: FATAL_LINE, n: 1, first: stoppedAt, last: stoppedAt }] : [];
      store.problems.value = empty ? [] : [...fatal, ...PROBLEMS(now)];
      store.batches.value = empty ? [] : INITIAL_BATCHES;
      store.topGames.value = empty ? [] : POOL;
      store.players.value = empty ? EMPTY_PLAYERS() : FULL_PLAYERS();
    }
    if (changed("conn")) {
      store.conn.value = d.conn === "lost" ? { state: "lost", attempt: 1, since: now } : { state: "live", attempt: 0, since: now };
    }
    if (changed("lookup") || changed("data")) {
      store.lookup.value = {
        enabled: d.lookup !== "off",
        state: d.lookup === "ratelimited" ? "ratelimited" : "ok",
        lastAt: empty ? null : now - 28 * SEC,
        lastCount: 37,
        queued: d.lookup === "ratelimited" ? 61 : 3,
        missingAll: 529,
        retryAt: now + 42 * SEC,
      };
    }
  });
}

export function setDemo(key, value) {
  const prev = demo.value;
  demo.value = { ...prev, [key]: value };
  apply(prev);
}

// ---------- fake event stream ----------

function resolveLookup(key, game) {
  const l = store.lookup.value;
  if (!store.batches.value.some((b) => b.key === key)) return;
  if (!l.enabled || l.state !== "ok") return setTimeout(() => resolveLookup(key, game), 1500);
  batch(() => {
    store.batches.value = store.batches.value.map((b) => (b.key === key ? { ...b, game } : b));
    if (game.white.ai && game.black.ai) return;
    const ps = [game.white, game.black].filter((p) => !p.ai);
    const titled = ps.some((p) => p.title) ? 1 : 0;
    const patron = ps.some((p) => p.patron) ? 1 : 0;
    const { today, all, byTitle } = store.players.value;
    store.players.value = {
      today: { looked: today.looked + 1, titled: today.titled + titled, patron: today.patron + patron },
      all: { ...all, looked: all.looked + 1, titled: all.titled + titled, patron: all.patron + patron },
      byTitle,
    };
  });
}

function newBatch() {
  const d = demo.value;
  if (d.state !== "running" || d.conn !== "live" || d.data === "empty") return;
  const now = store.now.value;
  const { b, game } = mockBatch(now, true);
  batch(() => {
    store.batches.value = [b, ...store.batches.value].slice(0, 9);
    const t = store.totals.value;
    store.totals.value = { ...t, batches: { ...t.batches, today: t.batches.today + 1, all: t.batches.all + 1 }, knps: { ...t.knps, last: b.knps } };
    store.client.value = { ...store.client.value, lastBatchAt: now, lastActivityAt: now };
  });
  setTimeout(() => resolveLookup(b.key, game), 3500);
}

let ticks = 0;
function tick() {
  ticks++;
  const now = store.now.value + SEC;
  const d = demo.value;
  batch(() => {
    store.now.value = now;
    const c = store.client.value;
    // An idle client logs "Going idle" again each time the wait runs out.
    if (c.state === "idle" && now >= c.idleUntil) {
      store.client.value = { ...c, idleUntil: now + 60 * SEC, lastActivityAt: now };
    }
    const l = store.lookup.value;
    if (l.state === "ratelimited" && now >= l.retryAt) store.lookup.value = { ...l, retryAt: now + 60 * SEC };
    if (d.conn === "lost" && ticks % 4 === 0) store.conn.value = { ...store.conn.value, attempt: store.conn.value.attempt + 1 };
  });
}

export function start() {
  store.now.value = START;
  apply(null);
  setInterval(tick, 1000);
  setInterval(newBatch, 6000);
}
