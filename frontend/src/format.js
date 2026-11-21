export const pad = (n) => String(n).padStart(2, "0");
export const fmt = (n) => n.toLocaleString("en-US");

export const hm = (t) => {
  const d = new Date(t);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
export const hms = (t) => `${hm(t)}:${pad(new Date(t).getSeconds())}`;

export const dayLabel = (t) =>
  new Date(t).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });

// Seconds as "42 s", "9 min 40 s", "3 h 12 min".
export const dur = (s) => {
  s = Math.max(0, Math.floor(s));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${pad(s % 60)} s`;
  return `${Math.floor(s / 3600)} h ${Math.floor(s / 60) % 60} min`;
};

export const uptime = (s) =>
  s < 86400 ? dur(s) : `${Math.floor(s / 86400)}d ${Math.floor(s / 3600) % 24}h ${Math.floor(s / 60) % 60}m`;

// Minutes as "45 m", "2 h", "1 h 10 m".
export const minutes = (m) =>
  m >= 60 ? `${Math.floor(m / 60)} h${m % 60 ? ` ${m % 60} m` : ""}` : `${m} m`;

// 18600 → "18.6k", 6.03e12 → "6.03T".
export const compact = (n) => {
  if (n < 1000) return fmt(n);
  const units = ["", "k", "M", "G", "T", "P"];
  const e = Math.min(units.length - 1, Math.floor(Math.log10(n) / 3));
  const v = n / 1000 ** e;
  return `${v.toFixed(v >= 100 ? 0 : v >= 10 ? 1 : 2)}${units[e]}`;
};

export const share = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : "—");

export const ago = (ms) => {
  const m = Math.round(ms / 60000);
  return m <= 0 ? "just now" : m < 60 ? `${m} min ago` : `${Math.round(m / 60)} h ago`;
};

const sameDay = (a, b) => new Date(a).toDateString() === new Date(b).toDateString();

// A past time relative to `ref`: "14:02:51", "yesterday 21:58:12", "19 Nov 08:00:00".
export const stamp = (t, ref) => {
  if (sameDay(t, ref)) return hms(t);
  if (sameDay(t, ref - 864e5)) return `yesterday ${hms(t)}`;
  return `${new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "short" })} ${hms(t)}`;
};

// Short form for table cells: time today, date otherwise.
export const when = (t, ref) =>
  ref - t < 864e5 ? hm(t) : new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
