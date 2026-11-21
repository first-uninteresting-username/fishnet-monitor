// The live data source: loads /api/status and /api/history once, then
// follows /api/events (Server-Sent Events). Each event carries the latest
// state of one part of the page (status, live, lookup, charts) and replaces
// the matching signals in store.js. The page never polls.
import { batch } from "@preact/signals";
import * as store from "./store.js";

// "today" is local midnight in the browser's time zone.
const query = `?tz=${new Date().getTimezoneOffset()}`;
const MAX_RETRY = 30000;

// When data last arrived, for the stale strip.
let lastData = Date.now();
// Highest batch key seen, so only new batches flash.
let maxKey = null;

function setStatus(s) {
  store.client.value = { ...store.client.value, ...s };
}

function setLive(d) {
  const prev = maxKey;
  for (const b of d.batches) maxKey = Math.max(maxKey ?? 0, b.key);
  store.totals.value = d.totals;
  store.batches.value = d.batches.map((b) => ({ ...b, live: prev != null && b.key > prev }));
  store.problems.value = d.problems;
}

function setLookup(d) {
  store.lookup.value = d.lookup;
  store.players.value = d.players;
  store.topGames.value = d.topGames;
}

function setCharts(d) {
  store.positions.value = d.positions;
  store.variants.value = d.variants;
}

const HANDLERS = { status: setStatus, live: setLive, lookup: setLookup, charts: setCharts };

async function load() {
  try {
    const get = (path) => fetch(path + query, { cache: "no-store" }).then((r) => r.json());
    const [status, history] = await Promise.all([get("api/status"), get("api/history")]);
    lastData = Date.now();
    batch(() => {
      setStatus(status);
      setLive(history);
      setLookup(history);
      setCharts(history);
    });
  } catch {
    // The event stream delivers the same data once it connects.
  }
}

function lost() {
  const c = store.conn.value;
  store.conn.value =
    c.state === "lost" ? { ...c, attempt: c.attempt + 1 } : { state: "lost", attempt: 1, since: lastData };
}

function connect() {
  const es = new EventSource(`api/events${query}`);
  es.onopen = () => (store.conn.value = { state: "live", attempt: 0, since: Date.now() });
  es.onerror = () => {
    lost();
    // The browser retries by itself unless the response was an error
    // (for example a proxy's 502); then retry here, backing off.
    if (es.readyState === EventSource.CLOSED) {
      const wait = Math.min(MAX_RETRY, 1000 * 2 ** store.conn.value.attempt);
      setTimeout(connect, wait);
    }
  };
  for (const [name, handle] of Object.entries(HANDLERS)) {
    es.addEventListener(name, (e) => {
      lastData = Date.now();
      batch(() => handle(JSON.parse(e.data)));
    });
  }
}

export function start() {
  setInterval(() => (store.now.value = Date.now()), 1000);
  load();
  connect();
}
