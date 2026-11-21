//! What the dashboard shows over time. No Prometheus is assumed, so the
//! monitor keeps its own history (see "History store" in `PLAN.md`):
//!
//! - **Minute buckets, 7 days:** batches, positions, nodes, speed, idle /
//!   stopped / quiet seconds, batches by variant and looked-up games. The
//!   charts up to 7 days, the "today" counts, the sparklines and the 24 h
//!   speed averages come from these.
//! - **Hour buckets, 7 weeks:** the same counts without speed and variants,
//!   for the 7-week chart.
//! - **Recent detail:** the last 200 batches, the last 200 `W:`/`E:` lines
//!   and the best games of each of the last 8 days.
//! - **All time, kept forever:** small aggregates only: counted batches,
//!   positions and nodes, batches by variant, game-lookup counts (titles,
//!   supporters, bots, missing) and the 50 highest-rated games.
//!
//! Everything older than its tier is dropped by [`History::prune`]. The
//! whole struct is the snapshot written to `HISTORY_FILE` (see
//! [`crate::snapshot`]). Times are epoch seconds in buckets and epoch
//! milliseconds in records (what the page uses).

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::parser::{BatchFinished, BatchTarget};

pub const MINUTE: u64 = 60;
pub const HOUR: u64 = 3600;
pub const DAY: u64 = 86400;
/// How long minute buckets are kept.
pub const MINUTES_KEPT: u64 = 7 * DAY;
/// How long hour buckets are kept.
pub const HOURS_KEPT: u64 = 7 * 7 * DAY;
pub const RECENT_BATCHES: usize = 200;
pub const PROBLEM_LINES: usize = 200;
/// All-time top games.
pub const TOP_ALL: usize = 50;
/// Best games kept per UTC day, for the 24 h / 7 d top lists.
pub const TOP_PER_DAY: usize = 20;
/// Days of per-day best games (one more than 7, so a rolling 7 d window
/// always has its oldest day).
pub const TOP_DAYS: u64 = 8;
/// After this long with no batch and no idle line, time counts as quiet
/// (matches `QUIET_AFTER` in `frontend/src/derived.js`).
pub const QUIET_AFTER: Duration = Duration::from_secs(180);
/// Problems shown on the page: the last 24 h.
pub const PROBLEM_WINDOW: u64 = DAY;
/// A gap between ticks longer than this (suspend, clock jump) isn't
/// attributed to any bucket.
const MAX_TICK_GAP: u64 = 120;

fn is_zero<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// One minute or one hour. Field names are short because the snapshot
/// holds about 11k of these.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    /// Start, epoch seconds.
    pub t: u64,
    #[serde(rename = "b", default, skip_serializing_if = "is_zero")]
    pub batches: u32,
    #[serde(rename = "p", default, skip_serializing_if = "is_zero")]
    pub positions: u64,
    #[serde(rename = "n", default, skip_serializing_if = "is_zero")]
    pub nodes: u64,
    /// Seconds spent idle (fishnet waiting for work).
    #[serde(rename = "i", default, skip_serializing_if = "is_zero")]
    pub idle: u32,
    /// Seconds with fishnet not running.
    #[serde(rename = "s", default, skip_serializing_if = "is_zero")]
    pub stopped: u32,
    /// Seconds running with no batch and no idle line for [`QUIET_AFTER`].
    #[serde(rename = "q", default, skip_serializing_if = "is_zero")]
    pub quiet: u32,
    /// Sum and count of knps/core of NNUE batches (minute buckets only).
    #[serde(rename = "ns", default, skip_serializing_if = "is_zero")]
    pub nnue_sum: u64,
    #[serde(rename = "nc", default, skip_serializing_if = "is_zero")]
    pub nnue_n: u32,
    #[serde(rename = "hs", default, skip_serializing_if = "is_zero")]
    pub hce_sum: u64,
    #[serde(rename = "hc", default, skip_serializing_if = "is_zero")]
    pub hce_n: u32,
    /// NNUE speed of the whole client from the summary line, in Mnps
    /// (minute buckets only).
    #[serde(rename = "m", default, skip_serializing_if = "Option::is_none")]
    pub mnps: Option<f64>,
    /// Games found by the lookup, with a titled player, with a supporter.
    #[serde(rename = "gl", default, skip_serializing_if = "is_zero")]
    pub looked: u32,
    #[serde(rename = "gt", default, skip_serializing_if = "is_zero")]
    pub titled: u32,
    #[serde(rename = "gp", default, skip_serializing_if = "is_zero")]
    pub patron: u32,
    /// Batches by `"<variant>:<eval>"` (minute buckets only).
    #[serde(rename = "v", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, u32>,
}

/// A player as the page shows it: a lichess account or the computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Player {
    Ai {
        ai: u8,
    },
    Human {
        name: String,
        rating: Option<u32>,
        title: Option<String>,
        #[serde(default)]
        patron: bool,
    },
}

impl Player {
    pub fn rating(&self) -> Option<u32> {
        match self {
            Player::Human { rating, .. } => *rating,
            Player::Ai { .. } => None,
        }
    }

    pub fn title(&self) -> Option<&str> {
        match self {
            Player::Human { title, .. } => title.as_deref(),
            Player::Ai { .. } => None,
        }
    }

    pub fn patron(&self) -> bool {
        matches!(self, Player::Human { patron: true, .. })
    }
}

/// A looked-up game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    /// When the batch finished (epoch ms).
    pub at: u64,
    pub white: Player,
    pub black: Player,
    /// Average of both ratings; `None` against the computer or when a side
    /// has no rating. Only games with a rating are ranked.
    pub rating: Option<u32>,
    pub speed: String,
    /// `None` for standard chess.
    pub variant: Option<String>,
    pub rated: bool,
}

impl Game {
    /// The rule from `PLAN.md`: the average of both players' ratings.
    pub fn average_rating(white: &Player, black: &Player) -> Option<u32> {
        Some((white.rating()? + black.rating()?).div_ceil(2))
    }
}

/// Where a batch's game lookup stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LookupMark {
    /// Queued for the lookup.
    Pending,
    /// Players known (`game` is set).
    Done,
    /// lichess didn't return the game.
    Missing,
    /// Not looked up: no game, another endpoint, or lookup off.
    Skipped,
}

/// The players of a batch's game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Players {
    pub white: Player,
    pub black: Player,
}

/// One finished batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRecord {
    pub key: u64,
    pub at: u64,
    /// The game id, or the batch id for batches without a game.
    pub id: String,
    /// The game's URL as fishnet printed it; `None` for batches without one.
    pub url: Option<String>,
    pub variant: String,
    pub eval: String,
    pub knps: Option<u32>,
    pub lookup: LookupMark,
    pub game: Option<Players>,
}

/// Level of a problem line. `F` is the line that made fishnet fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProblemLevel {
    W,
    E,
    F,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProblemLine {
    pub lvl: ProblemLevel,
    pub msg: String,
    pub at: u64,
}

/// Lines that differ only in numbers, grouped (what the page lists).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProblemGroup {
    pub lvl: ProblemLevel,
    /// The latest line of the group.
    pub msg: String,
    pub n: u32,
    pub first: u64,
    pub last: u64,
}

/// Aggregates that never expire.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AllTime {
    /// When this history began (epoch ms).
    pub since: u64,
    /// Batch lines seen by the monitor (fishnet's own total is in the stats
    /// file).
    pub batches: u64,
    pub positions: u64,
    pub nodes: u64,
    /// Batches by `"<variant>:<eval>"`.
    pub variants: BTreeMap<String, u64>,
    /// Games found by the lookup.
    pub looked: u64,
    /// Games lichess didn't return.
    pub missing: u64,
    /// Games with at least one titled player (`BOT` doesn't count).
    pub titled: u64,
    pub patron: u64,
    /// Games with a bot account.
    pub bots: u64,
    /// Games per title; a GM-vs-IM game counts for both.
    pub by_title: BTreeMap<String, u64>,
    /// The highest-rated games, best first.
    pub top: Vec<Game>,
}

/// What fishnet was doing during a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Busy,
    Idle,
    Quiet,
    Stopped,
}

/// A chart series, the shape of `positions[range]` in `store.js`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    /// Seconds per point.
    pub step: u64,
    /// End of each point's bucket, epoch seconds.
    pub t: Vec<u64>,
    /// Positions per minute (average over the bucket).
    pub v: Vec<f64>,
    /// `busy`, `idle`, `stop` or `quiet`.
    pub kind: Vec<&'static str>,
}

/// One chart range: points, seconds per point, and whether it reads the
/// hour buckets.
struct Range {
    name: &'static str,
    points: u64,
    step: u64,
    hours: bool,
}

const RANGES: [Range; 5] = [
    Range {
        name: "1h",
        points: 60,
        step: MINUTE,
        hours: false,
    },
    Range {
        name: "6h",
        points: 72,
        step: 5 * MINUTE,
        hours: false,
    },
    Range {
        name: "24h",
        points: 288,
        step: 5 * MINUTE,
        hours: false,
    },
    Range {
        name: "7d",
        points: 336,
        step: 30 * MINUTE,
        hours: false,
    },
    Range {
        name: "7w",
        points: 294,
        step: 4 * HOUR,
        hours: true,
    },
];

/// Sparkline points over the last 24 h (30 minutes each).
pub const SPARK_POINTS: u64 = 48;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub minutes: VecDeque<Bucket>,
    pub hours: VecDeque<Bucket>,
    pub batches: VecDeque<BatchRecord>,
    pub next_key: u64,
    pub problems: VecDeque<ProblemLine>,
    /// Best games of each of the last [`TOP_DAYS`] UTC days.
    pub recent_top: Vec<Game>,
    pub all: AllTime,
    /// Last [`History::tick`], epoch seconds. Not saved: time while the
    /// monitor was down counts as stopped anyway.
    #[serde(skip)]
    last_tick: Option<u64>,
}

pub fn secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn millis(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The bucket starting at `start` in `deque`, created if it's newer than the
/// last one. A time before the last bucket (the clock went back) uses the
/// last bucket.
fn slot(deque: &mut VecDeque<Bucket>, start: u64) -> &mut Bucket {
    let push = deque.back().is_none_or(|b| b.t < start);
    if push {
        deque.push_back(Bucket {
            t: start,
            ..Bucket::default()
        });
    }
    deque.back_mut().expect("not empty")
}

/// Replaces runs of digits with `#`, so lines that differ only in numbers
/// group together.
fn problem_key(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut in_digits = false;
    for c in msg.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

/// Local midnight (epoch seconds) for a browser's `Date.getTimezoneOffset()`
/// (minutes, positive west of UTC).
pub fn local_midnight(now_s: u64, tz_offset_min: i32) -> u64 {
    let offset = i64::from(tz_offset_min) * 60;
    let local = now_s as i64 - offset;
    let midnight = local - local.rem_euclid(DAY as i64);
    (midnight + offset).max(0) as u64
}

impl History {
    pub fn new(now: SystemTime) -> History {
        History {
            all: AllTime {
                since: millis(now),
                ..AllTime::default()
            },
            ..History::default()
        }
    }

    /// Applies `f` to the current minute and hour bucket.
    fn add(&mut self, now_s: u64, f: impl Fn(&mut Bucket)) {
        f(slot(&mut self.minutes, now_s - now_s % MINUTE));
        f(slot(&mut self.hours, now_s - now_s % HOUR));
    }

    fn minute(&mut self, now_s: u64) -> &mut Bucket {
        slot(&mut self.minutes, now_s - now_s % MINUTE)
    }

    /// A finished batch. Returns the record's key.
    pub fn record_batch(&mut self, now: SystemTime, b: &BatchFinished, lookup: LookupMark) -> u64 {
        let now_s = secs(now);
        let variant = b.variant.as_str().to_owned();
        let eval = b.eval.as_str().to_owned();
        let label = format!("{variant}:{eval}");

        self.add(now_s, |m| m.batches += 1);
        let minute = self.minute(now_s);
        *minute.variants.entry(label.clone()).or_insert(0) += 1;
        if let Some(knps) = b.knps_per_core {
            match b.eval {
                crate::parser::Eval::Nnue => {
                    minute.nnue_sum += u64::from(knps);
                    minute.nnue_n += 1;
                }
                crate::parser::Eval::Hce => {
                    minute.hce_sum += u64::from(knps);
                    minute.hce_n += 1;
                }
            }
        }
        self.all.batches += 1;
        *self.all.variants.entry(label).or_insert(0) += 1;

        let (id, url) = match &b.target {
            BatchTarget::Game(game) => (game.game_id.clone(), Some(game.url.clone())),
            BatchTarget::Batch(id) => (id.clone(), None),
        };
        self.next_key += 1;
        let key = self.next_key;
        self.batches.push_front(BatchRecord {
            key,
            at: millis(now),
            id,
            url,
            variant,
            eval,
            knps: b.knps_per_core,
            lookup,
            game: None,
        });
        self.batches.truncate(RECENT_BATCHES);
        key
    }

    /// Positions and nodes analysed since the last reading of fishnet's
    /// counters.
    pub fn record_work(&mut self, now: SystemTime, positions: u64, nodes: u64) {
        if positions == 0 && nodes == 0 {
            return;
        }
        self.add(secs(now), |m| {
            m.positions += positions;
            m.nodes += nodes;
        });
        self.all.positions += positions;
        self.all.nodes += nodes;
    }

    /// The NNUE speed from the summary line, in Mnps for the whole client.
    pub fn record_speed(&mut self, now: SystemTime, mnps: f64) {
        self.minute(secs(now)).mnps = Some(mnps);
    }

    pub fn record_problem(&mut self, now: SystemTime, lvl: ProblemLevel, msg: &str) {
        self.problems.push_front(ProblemLine {
            lvl,
            msg: msg.to_owned(),
            at: millis(now),
        });
        self.problems.truncate(PROBLEM_LINES);
    }

    /// Called every few seconds. Attributes the time since the last tick to
    /// the current bucket and drops expired data. Returns true when a new
    /// minute began (the charts have a new point).
    pub fn tick(&mut self, now: SystemTime, activity: Activity) -> bool {
        let now_s = secs(now);
        let closed = self
            .last_tick
            .is_none_or(|prev| prev / MINUTE != now_s / MINUTE);
        if let Some(prev) = self.last_tick {
            let dt = now_s.saturating_sub(prev);
            if dt <= MAX_TICK_GAP {
                let dt = dt as u32;
                self.add(now_s, |m| match activity {
                    Activity::Busy => {}
                    Activity::Idle => m.idle += dt,
                    Activity::Quiet => m.quiet += dt,
                    Activity::Stopped => m.stopped += dt,
                });
            }
        }
        // Keep one bucket per minute even when nothing happens, so the
        // charts can tell "monitor running" from "monitor down".
        self.add(now_s, |_| {});
        self.last_tick = Some(now_s);
        self.prune(now_s);
        closed
    }

    /// Drops buckets and records older than their tier.
    pub fn prune(&mut self, now_s: u64) {
        let minute_cut = now_s.saturating_sub(MINUTES_KEPT);
        while self.minutes.front().is_some_and(|b| b.t < minute_cut) {
            self.minutes.pop_front();
        }
        let hour_cut = now_s.saturating_sub(HOURS_KEPT);
        while self.hours.front().is_some_and(|b| b.t < hour_cut) {
            self.hours.pop_front();
        }
        let day_cut = (now_s / DAY).saturating_sub(TOP_DAYS - 1) * DAY * 1000;
        self.recent_top.retain(|g| g.at >= day_cut);
    }

    // ---------- game lookup ----------

    /// Sets the lookup result on the batches of `id`.
    pub fn mark_batches(&mut self, id: &str, mark: LookupMark, players: Option<&Players>) {
        for b in self.batches.iter_mut().filter(|b| b.id == id) {
            if b.lookup == LookupMark::Pending {
                b.lookup = mark;
                b.game = players.cloned();
            }
        }
    }

    /// A game lichess didn't return.
    pub fn record_missing(&mut self, id: &str) {
        self.all.missing += 1;
        self.mark_batches(id, LookupMark::Missing, None);
    }

    /// A looked-up game: counts it, ranks it, and fills in its batches.
    pub fn record_game(&mut self, now: SystemTime, game: Game) {
        let players = Players {
            white: game.white.clone(),
            black: game.black.clone(),
        };
        self.mark_batches(&game.id, LookupMark::Done, Some(&players));

        let sides = [&game.white, &game.black];
        let titles: BTreeSet<&str> = sides
            .iter()
            .filter_map(|p| p.title())
            .filter(|t| *t != "BOT")
            .collect();
        let bot = sides.iter().any(|p| p.title() == Some("BOT"));
        let patron = sides.iter().any(|p| p.patron());
        let titled = !titles.is_empty();

        self.add(secs(now), |m| {
            m.looked += 1;
            m.titled += u32::from(titled);
            m.patron += u32::from(patron);
        });
        self.all.looked += 1;
        self.all.titled += u64::from(titled);
        self.all.patron += u64::from(patron);
        self.all.bots += u64::from(bot);
        for title in titles {
            *self.all.by_title.entry(title.to_owned()).or_insert(0) += 1;
        }

        if game.rating.is_some() {
            self.rank(game);
        }
    }

    /// Adds a ranked game to the all-time and per-day top lists. A game
    /// already listed (analysed twice) is skipped.
    fn rank(&mut self, game: Game) {
        let rating = game.rating.unwrap_or(0);
        let listed = |list: &[Game]| list.iter().any(|g| g.id == game.id);

        if !listed(&self.all.top) {
            let pos = self
                .all
                .top
                .partition_point(|g| g.rating.unwrap_or(0) >= rating);
            if pos < TOP_ALL {
                self.all.top.insert(pos, game.clone());
                self.all.top.truncate(TOP_ALL);
            }
        }

        if !listed(&self.recent_top) {
            let day = game.at / 1000 / DAY;
            self.recent_top.push(game);
            let mut same_day: Vec<usize> = (0..self.recent_top.len())
                .filter(|&i| self.recent_top[i].at / 1000 / DAY == day)
                .collect();
            if same_day.len() > TOP_PER_DAY {
                same_day.sort_by_key(|&i| self.recent_top[i].rating);
                self.recent_top.remove(same_day[0]);
            }
        }
    }

    // ---------- views for the page ----------

    /// All chart ranges, keyed like `positions` in `store.js`.
    pub fn charts(&self, now: SystemTime) -> BTreeMap<&'static str, Series> {
        RANGES
            .iter()
            .map(|r| (r.name, self.series(secs(now), r)))
            .collect()
    }

    fn series(&self, now_s: u64, range: &Range) -> Series {
        let (source, unit) = if range.hours {
            (&self.hours, HOUR)
        } else {
            (&self.minutes, MINUTE)
        };
        let since = self.all.since / 1000;
        let last = now_s - now_s % range.step;
        let first = last.saturating_sub((range.points - 1) * range.step);
        // Leave out the time before the history began.
        let first = first.max(since - since % range.step);

        let mut series = Series {
            step: range.step,
            t: Vec::new(),
            v: Vec::new(),
            kind: Vec::new(),
        };
        let mut idx = source.partition_point(|b| b.t < first);
        let mut start = first;
        while start <= last {
            let end = start + range.step;
            let (mut positions, mut idle, mut stopped, mut quiet) = (0u64, 0u64, 0u64, 0u64);
            let mut covered = 0u64;
            while let Some(b) = source.get(idx).filter(|b| b.t < end) {
                positions += b.positions;
                idle += u64::from(b.idle);
                stopped += u64::from(b.stopped);
                quiet += u64::from(b.quiet);
                covered += unit;
                idx += 1;
            }
            // Time inside the history with no bucket: the monitor (and so
            // fishnet) wasn't running.
            let span_end = end.min(now_s - now_s % unit + unit);
            let span_start = start.max(since - since % unit);
            stopped += span_end.saturating_sub(span_start).saturating_sub(covered);

            let per_minute = positions as f64 / (range.step / MINUTE) as f64;
            let kind = if positions > 0 {
                "busy"
            } else if stopped >= idle.max(quiet) && stopped > 0 {
                "stop"
            } else if quiet > idle {
                "quiet"
            } else {
                "idle"
            };
            series.t.push(end);
            series.v.push((per_minute * 10.0).round() / 10.0);
            series.kind.push(kind);
            start = end;
        }
        series
    }

    /// Minute buckets in `[from, to)`.
    fn minutes_between(&self, from: u64, to: u64) -> impl Iterator<Item = &Bucket> {
        let lo = self.minutes.partition_point(|b| b.t < from);
        self.minutes.range(lo..).take_while(move |b| b.t < to)
    }

    /// Sums since `from`.
    pub fn sum_since(&self, from: u64) -> Bucket {
        let mut sum = Bucket::default();
        for b in self.minutes_between(from, u64::MAX) {
            sum.batches += b.batches;
            sum.positions += b.positions;
            sum.nodes += b.nodes;
            sum.nnue_sum += b.nnue_sum;
            sum.nnue_n += b.nnue_n;
            sum.hce_sum += b.hce_sum;
            sum.hce_n += b.hce_n;
            sum.looked += b.looked;
            sum.titled += b.titled;
            sum.patron += b.patron;
        }
        sum
    }

    /// 24 h of 30-minute points for the tile sparklines, oldest first:
    /// batches, positions, nodes, average knps/core, Mnps. Speed points are
    /// left out where there was no batch / summary.
    pub fn sparks(&self, now: SystemTime) -> Sparks {
        let now_s = secs(now);
        let step = DAY / SPARK_POINTS;
        let last = now_s - now_s % step;
        let mut sparks = Sparks::default();
        for i in (0..SPARK_POINTS).rev() {
            let from = last - i * step;
            let (mut batches, mut positions, mut nodes) = (0u64, 0u64, 0u64);
            let (mut knps_sum, mut knps_n) = (0u64, 0u64);
            let (mut mnps_sum, mut mnps_n) = (0f64, 0u32);
            for b in self.minutes_between(from, from + step) {
                batches += u64::from(b.batches);
                positions += b.positions;
                nodes += b.nodes;
                knps_sum += b.nnue_sum + b.hce_sum;
                knps_n += u64::from(b.nnue_n + b.hce_n);
                if let Some(m) = b.mnps {
                    mnps_sum += m;
                    mnps_n += 1;
                }
            }
            sparks.batches.push(batches as f64);
            sparks.positions.push(positions as f64);
            sparks.nodes.push(nodes as f64);
            if knps_n > 0 {
                sparks.knps.push(knps_sum as f64 / knps_n as f64);
            }
            if mnps_n > 0 {
                sparks.mnps.push(mnps_sum / f64::from(mnps_n));
            }
        }
        sparks
    }

    /// Batches by variant over the last 7 days, most first.
    pub fn variants(&self, now: SystemTime) -> Vec<VariantCount> {
        let mut counts: HashMap<&str, u64> = HashMap::new();
        for b in self.minutes_between(secs(now).saturating_sub(7 * DAY), u64::MAX) {
            for (label, n) in &b.variants {
                *counts.entry(label).or_insert(0) += u64::from(*n);
            }
        }
        let mut list: Vec<VariantCount> = counts
            .into_iter()
            .map(|(label, n)| {
                let (name, eval) = label.split_once(':').unwrap_or((label, "nnue"));
                VariantCount {
                    name: name.to_owned(),
                    n,
                    eval: eval.to_owned(),
                }
            })
            .collect();
        list.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.name.cmp(&b.name)));
        list
    }

    /// Problem lines of the last 24 h, grouped, most recent first.
    pub fn problem_groups(&self, now: SystemTime) -> Vec<ProblemGroup> {
        let cut = millis(now).saturating_sub(PROBLEM_WINDOW * 1000);
        let mut groups: Vec<(String, ProblemGroup)> = Vec::new();
        // Newest first, so the first line of a group is its latest.
        for p in self.problems.iter().filter(|p| p.at >= cut) {
            let key = problem_key(&p.msg);
            match groups.iter_mut().find(|(k, g)| *k == key && g.lvl == p.lvl) {
                Some((_, g)) => {
                    g.n += 1;
                    g.first = g.first.min(p.at);
                }
                None => groups.push((
                    key,
                    ProblemGroup {
                        lvl: p.lvl,
                        msg: p.msg.clone(),
                        n: 1,
                        first: p.at,
                        last: p.at,
                    },
                )),
            }
        }
        groups.into_iter().map(|(_, g)| g).collect()
    }

    /// The all-time and per-day best games, deduplicated, best first. The
    /// page filters them by period.
    pub fn top_games(&self) -> Vec<Game> {
        let mut games = self.all.top.clone();
        for g in &self.recent_top {
            if !games.iter().any(|x| x.id == g.id) {
                games.push(g.clone());
            }
        }
        games.sort_by_key(|g| std::cmp::Reverse(g.rating));
        games
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Sparks {
    pub batches: Vec<f64>,
    pub positions: Vec<f64>,
    pub nodes: Vec<f64>,
    pub knps: Vec<f64>,
    pub mnps: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantCount {
    pub name: String,
    pub n: u64,
    pub eval: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Event, parse_line};

    /// 2026-11-21 00:00:00 UTC.
    const T0: u64 = 1_795_219_200;

    fn at(s: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(s)
    }

    fn batch(line: &str) -> BatchFinished {
        match parse_line(line).event {
            Event::BatchFinished(b) => b,
            other => panic!("not a batch line: {other:?}"),
        }
    }

    const NNUE: &str = "[    |            ] https://lichess.org/AbCd1234 finished (1500 knps/core)";
    const HCE: &str =
        "[    |            ] https://lichess.org/EfGh5678 finished (atomic, hce, 900 knps/core)";
    const NO_GAME: &str = "[    |            ] batch Xy12 finished (1200 knps/core)";

    fn human(name: &str, rating: u32, title: Option<&str>, patron: bool) -> Player {
        Player::Human {
            name: name.into(),
            rating: Some(rating),
            title: title.map(Into::into),
            patron,
        }
    }

    fn game(id: &str, at_s: u64, white: Player, black: Player) -> Game {
        Game {
            id: id.into(),
            at: at_s * 1000,
            rating: Game::average_rating(&white, &black),
            white,
            black,
            speed: "blitz".into(),
            variant: None,
            rated: true,
        }
    }

    #[test]
    fn batches_go_into_minute_and_hour_buckets() {
        let mut h = History::new(at(T0));
        h.record_batch(at(T0 + 10), &batch(NNUE), LookupMark::Pending);
        h.record_batch(at(T0 + 50), &batch(HCE), LookupMark::Skipped);
        h.record_batch(at(T0 + 70), &batch(NO_GAME), LookupMark::Skipped);

        assert_eq!(h.minutes.len(), 2);
        assert_eq!(h.minutes[0].batches, 2);
        assert_eq!(h.minutes[0].nnue_sum, 1500);
        assert_eq!(h.minutes[0].hce_n, 1);
        assert_eq!(h.minutes[0].variants["atomic:hce"], 1);
        assert_eq!(h.hours.len(), 1);
        assert_eq!(h.hours[0].batches, 3);
        assert_eq!(h.all.batches, 3);
        assert_eq!(h.all.variants["standard:nnue"], 2);

        // Newest first; batches without a game keep their batch id.
        assert_eq!(h.batches[0].id, "Xy12");
        assert_eq!(h.batches[0].url, None);
        assert_eq!(h.batches[2].id, "AbCd1234");
        assert_eq!(
            h.batches[2].url.as_deref(),
            Some("https://lichess.org/AbCd1234")
        );
        assert_eq!(h.batches[0].key, 3);
    }

    #[test]
    fn recent_batches_and_problems_are_capped() {
        let mut h = History::new(at(T0));
        for i in 0..(RECENT_BATCHES as u64 + 5) {
            h.record_batch(at(T0 + i), &batch(NNUE), LookupMark::Skipped);
            h.record_problem(at(T0 + i), ProblemLevel::W, "x");
        }
        assert_eq!(h.batches.len(), RECENT_BATCHES);
        assert_eq!(h.problems.len(), PROBLEM_LINES);
    }

    #[test]
    fn ticks_attribute_time_by_activity() {
        let mut h = History::new(at(T0));
        assert!(h.tick(at(T0), Activity::Busy));
        assert!(!h.tick(at(T0 + 5), Activity::Idle));
        assert!(!h.tick(at(T0 + 10), Activity::Stopped));
        assert!(!h.tick(at(T0 + 15), Activity::Quiet));
        assert!(h.tick(at(T0 + 60), Activity::Busy));
        assert_eq!(h.minutes.len(), 2);
        assert_eq!(
            (h.minutes[0].idle, h.minutes[0].stopped, h.minutes[0].quiet),
            (5, 5, 5)
        );
        assert_eq!(h.hours[0].idle, 5);
        // A long gap (suspend) isn't attributed.
        h.tick(at(T0 + 1000), Activity::Idle);
        assert_eq!(h.minutes.back().unwrap().idle, 0);
    }

    #[test]
    fn prune_keeps_minutes_7_days_and_hours_7_weeks() {
        let mut h = History::new(at(T0));
        h.record_work(at(T0), 10, 100);
        let later = T0 + 8 * DAY;
        h.tick(at(later), Activity::Busy);
        assert_eq!(h.minutes.len(), 1, "old minute dropped");
        assert_eq!(h.hours.len(), 2, "hour still kept");
        h.tick(at(T0 + 50 * DAY), Activity::Busy);
        assert_eq!(h.hours.front().unwrap().t, later, "old hour dropped");
        // All-time counts never expire.
        assert_eq!(h.all.positions, 10);
        assert_eq!(h.all.nodes, 100);
    }

    #[test]
    fn series_covers_ranges_and_marks_gaps_as_stopped() {
        let mut h = History::new(at(T0));
        h.tick(at(T0), Activity::Busy);
        h.record_work(at(T0 + 30), 120, 0);
        h.tick(at(T0 + 60), Activity::Busy);
        h.tick(at(T0 + 65), Activity::Idle);
        h.tick(at(T0 + 120), Activity::Idle);
        // Monitor down for minutes 3 and 4, back at minute 5.
        h.tick(at(T0 + 300), Activity::Busy);
        let charts = h.charts(at(T0 + 330));

        let s = &charts["1h"];
        assert_eq!(s.step, 60);
        // The history began at T0, so the series starts there.
        assert_eq!(s.t.first(), Some(&(T0 + 60)));
        assert_eq!(s.t.last(), Some(&(T0 + 360)));
        assert_eq!(s.v[0], 120.0);
        assert_eq!(
            s.kind,
            vec!["busy", "idle", "idle", "stop", "stop", "idle"],
            "{s:?}"
        );

        let s = &charts["6h"];
        assert_eq!(s.step, 300);
        assert_eq!(s.v[0], 24.0, "per-minute average over 5 minutes");
        assert_eq!(s.kind[0], "busy");

        assert_eq!(charts["7w"].step, 4 * HOUR);
        assert_eq!(charts.len(), 5);
    }

    #[test]
    fn series_is_capped_to_the_range() {
        let mut h = History::new(at(T0));
        h.all.since = 0;
        let s = &h.charts(at(T0 + 10 * DAY))["1h"];
        assert_eq!(s.t.len(), 60);
        assert!(s.kind.iter().all(|k| *k == "stop"));
    }

    #[test]
    fn local_midnight_follows_the_browser_offset() {
        let noon = T0 + 12 * HOUR;
        assert_eq!(local_midnight(noon, 0), T0);
        // UTC+1 (offset -60): local midnight is 23:00 UTC the day before.
        assert_eq!(local_midnight(noon, -60), T0 - HOUR);
        // UTC-5 (offset 300): 05:00 UTC.
        assert_eq!(local_midnight(noon, 300), T0 + 5 * HOUR);
        // 02:00 UTC is still the previous local day in UTC-5.
        assert_eq!(local_midnight(T0 + 2 * HOUR, 300), T0 + 5 * HOUR - DAY);
    }

    #[test]
    fn sum_since_and_sparks() {
        let mut h = History::new(at(T0));
        h.record_batch(at(T0 + 10), &batch(NNUE), LookupMark::Skipped);
        h.record_batch(at(T0 + 4000), &batch(HCE), LookupMark::Skipped);
        h.record_work(at(T0 + 4000), 50, 5000);
        h.record_speed(at(T0 + 4000), 12.5);

        let sum = h.sum_since(T0 + 3600);
        assert_eq!((sum.batches, sum.positions, sum.nodes), (1, 50, 5000));

        let sparks = h.sparks(at(T0 + 5000));
        assert_eq!(sparks.batches.len(), SPARK_POINTS as usize);
        assert_eq!(sparks.batches.iter().sum::<f64>(), 2.0);
        assert_eq!(sparks.knps, vec![1500.0, 900.0]);
        assert_eq!(sparks.mnps, vec![12.5]);
    }

    #[test]
    fn variants_of_the_last_7_days() {
        let mut h = History::new(at(T0));
        h.record_batch(at(T0), &batch(HCE), LookupMark::Skipped);
        h.record_batch(at(T0 + 1), &batch(NNUE), LookupMark::Skipped);
        h.record_batch(at(T0 + 2), &batch(NNUE), LookupMark::Skipped);
        let v = h.variants(at(T0 + 100));
        assert_eq!(v[0].name, "standard");
        assert_eq!((v[0].n, v[0].eval.as_str()), (2, "nnue"));
        assert_eq!((v[1].name.as_str(), v[1].eval.as_str()), ("atomic", "hce"));
        assert!(h.variants(at(T0 + 8 * DAY)).is_empty());
    }

    #[test]
    fn problems_are_grouped_by_text_without_numbers() {
        let mut h = History::new(at(T0));
        h.record_problem(at(T0), ProblemLevel::W, "Backing off 12s.");
        h.record_problem(at(T0 + 10), ProblemLevel::W, "Backing off 30s.");
        h.record_problem(at(T0 + 20), ProblemLevel::E, "Backing off 30s.");
        h.record_problem(at(T0 + 30), ProblemLevel::F, "Stopping queue.");
        let groups = h.problem_groups(at(T0 + 40));
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].lvl, ProblemLevel::F);
        let w = groups.iter().find(|g| g.lvl == ProblemLevel::W).unwrap();
        assert_eq!(w.n, 2);
        assert_eq!(w.msg, "Backing off 30s.");
        assert_eq!((w.first, w.last), (T0 * 1000, (T0 + 10) * 1000));
        // Only the last 24 h.
        assert_eq!(h.problem_groups(at(T0 + 2 * DAY)).len(), 0);
    }

    #[test]
    fn games_are_counted_and_ranked() {
        let mut h = History::new(at(T0));
        h.record_batch(at(T0), &batch(NNUE), LookupMark::Pending);
        let g = game(
            "AbCd1234",
            T0,
            human("a", 2500, Some("GM"), false),
            human("b", 2401, Some("IM"), true),
        );
        h.record_game(at(T0 + 60), g);
        assert_eq!(h.batches[0].lookup, LookupMark::Done);
        assert!(h.batches[0].game.is_some());
        assert_eq!(h.all.top[0].rating, Some(2451));
        assert_eq!((h.all.looked, h.all.titled, h.all.patron), (1, 1, 1));
        assert_eq!(h.all.by_title["GM"], 1);
        assert_eq!(h.all.by_title["IM"], 1);

        // Two GMs: one titled game, one GM count.
        h.record_game(
            at(T0 + 60),
            game(
                "x2",
                T0,
                human("c", 2600, Some("GM"), false),
                human("d", 2600, Some("GM"), false),
            ),
        );
        assert_eq!(h.all.by_title["GM"], 2);
        assert_eq!(h.all.top[0].id, "x2");

        // A bot game counts as a bot game, not a titled one.
        h.record_game(
            at(T0 + 60),
            game(
                "x3",
                T0,
                human("e", 2000, Some("BOT"), false),
                human("f", 2000, None, false),
            ),
        );
        assert_eq!((h.all.titled, h.all.bots), (2, 1));
        assert!(!h.all.by_title.contains_key("BOT"));

        // Against the computer: counted, not ranked.
        h.record_game(
            at(T0 + 60),
            game(
                "x4",
                T0,
                human("g", 1500, None, false),
                Player::Ai { ai: 5 },
            ),
        );
        assert_eq!(h.all.looked, 4);
        assert!(h.top_games().iter().all(|g| g.id != "x4"));

        // A game analysed twice is ranked once.
        h.record_game(
            at(T0 + 60),
            game(
                "x2",
                T0,
                human("c", 2600, Some("GM"), false),
                human("d", 2600, Some("GM"), false),
            ),
        );
        assert_eq!(h.top_games().iter().filter(|g| g.id == "x2").count(), 1);

        h.record_missing("zzz");
        assert_eq!(h.all.missing, 1);
        assert_eq!(h.minutes.back().unwrap().looked, 5);
    }

    #[test]
    fn top_lists_are_capped() {
        let mut h = History::new(at(T0));
        for i in 0..(TOP_ALL as u32 + 10) {
            let p = |n| human(n, 1000 + i, None, false);
            h.record_game(at(T0), game(&format!("g{i}"), T0, p("a"), p("b")));
        }
        assert_eq!(h.all.top.len(), TOP_ALL);
        assert_eq!(h.all.top[0].rating, Some(1000 + TOP_ALL as u32 + 9));
        assert_eq!(h.recent_top.len(), TOP_PER_DAY);
        assert!(
            h.recent_top
                .iter()
                .all(|g| g.rating >= Some(1000 + TOP_ALL as u32 + 10 - TOP_PER_DAY as u32))
        );
        // Per-day games go after 8 days; all-time ones stay.
        h.prune(T0 + 9 * DAY);
        assert!(h.recent_top.is_empty());
        assert_eq!(h.all.top.len(), TOP_ALL);
    }

    #[test]
    fn players_serialize_like_the_page_expects() {
        let ai = serde_json::to_value(Player::Ai { ai: 6 }).unwrap();
        assert_eq!(ai, serde_json::json!({"ai": 6}));
        let p = serde_json::to_value(human("n", 2000, Some("FM"), true)).unwrap();
        assert_eq!(
            p,
            serde_json::json!({"name": "n", "rating": 2000, "title": "FM", "patron": true})
        );
        let back: Player = serde_json::from_value(p).unwrap();
        assert_eq!(back, human("n", 2000, Some("FM"), true));
    }
}
