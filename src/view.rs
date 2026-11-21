//! The JSON the dashboard reads, shaped like the signals in
//! `frontend/src/store.js`. `/api/history` returns all parts at once;
//! `/api/events` sends each part as its own event when it changes (see
//! [`Topic`]).

use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::SystemTime,
};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;

use crate::{
    history::{BatchRecord, DAY, Game, ProblemGroup, Series, VariantCount, local_midnight, secs},
    state::{State, StatusContext, TotalsSource},
};

/// Recent batches sent to the page.
pub const PAGE_BATCHES: usize = 12;

/// The parts of the page that change independently. Each is one SSE event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    /// `/api/status`: the header and the strips.
    Status,
    /// Tiles, recent batches and problems: changes with every batch.
    Live,
    /// Game lookup status, player counts and top games.
    Lookup,
    /// Positions chart and variants: changes once a minute.
    Charts,
}

impl Topic {
    pub const ALL: [Topic; 4] = [Topic::Status, Topic::Live, Topic::Lookup, Topic::Charts];

    pub fn as_str(self) -> &'static str {
        match self {
            Topic::Status => "status",
            Topic::Live => "live",
            Topic::Lookup => "lookup",
            Topic::Charts => "charts",
        }
    }
}

/// Tells open `/api/events` streams which parts changed. Each topic is a
/// `watch` counter, so a slow page never queues up events: it just sends
/// the latest state of what changed.
#[derive(Debug)]
pub struct Changes {
    senders: [watch::Sender<u64>; 4],
}

impl Default for Changes {
    fn default() -> Self {
        Changes {
            senders: std::array::from_fn(|_| watch::channel(0).0),
        }
    }
}

impl Changes {
    pub fn bump(&self, topic: Topic) {
        self.senders[topic as usize].send_modify(|n| *n += 1);
    }

    pub fn bump_all(&self, topics: &[Topic]) {
        for topic in topics {
            self.bump(*topic);
        }
    }

    pub fn subscribe(&self) -> [watch::Receiver<u64>; 4] {
        std::array::from_fn(|i| self.senders[i].subscribe())
    }
}

/// The page's time zone: `Date.getTimezoneOffset()`, for "today".
#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
pub struct Tz {
    #[serde(default)]
    pub tz: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Count {
    pub today: u64,
    pub all: u64,
    pub spark: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Knps {
    /// The last batch.
    pub last: Option<u32>,
    /// 24 h averages by engine.
    pub nnue: Option<u32>,
    pub hce: Option<u32>,
    pub spark: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Nnue {
    pub mnps: Option<f64>,
    pub spark: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Totals {
    pub batches: Count,
    pub positions: Count,
    pub nodes: Count,
    pub knps: Knps,
    pub nnue: Nnue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Live {
    pub totals: Totals,
    pub batches: Vec<BatchRecord>,
    pub problems: Vec<ProblemGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerCounts {
    pub looked: u64,
    pub titled: u64,
    pub patron: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bots: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayersView {
    pub today: PlayerCounts,
    pub all: PlayerCounts,
    /// `[title, games]`, strongest title first.
    pub by_title: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupPart {
    pub lookup: Value,
    pub players: PlayersView,
    pub top_games: Vec<Game>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Charts {
    pub positions: std::collections::BTreeMap<&'static str, Series>,
    pub variants: Vec<VariantCount>,
}

/// Title order on the page: lichess's titles, strongest first.
const TITLE_ORDER: [&str; 11] = [
    "GM", "IM", "FM", "CM", "NM", "WGM", "WIM", "WFM", "WCM", "WNM", "LM",
];

fn avg(sum: u64, n: u32) -> Option<u32> {
    (n > 0).then(|| (sum as f64 / f64::from(n)).round() as u32)
}

pub fn totals(state: &State, now: SystemTime, tz: Tz) -> Totals {
    let h = &state.history;
    let now_s = secs(now);
    let today = h.sum_since(local_midnight(now_s, tz.tz));
    let day = h.sum_since(now_s.saturating_sub(DAY));
    let sparks = h.sparks(now);
    // fishnet's own totals when known; the monitor's count otherwise.
    let (batches, positions, nodes) = if state.totals_source == TotalsSource::None {
        (h.all.batches, h.all.positions, h.all.nodes)
    } else {
        (
            state.totals.batches,
            state.totals.positions,
            state.totals.nodes,
        )
    };
    let mnps = match (state.nnue_knps_per_core, state.cores) {
        (Some(knps), Some(cores)) => Some(f64::from(knps) * f64::from(cores) / 1000.0),
        _ => None,
    };
    Totals {
        batches: Count {
            today: u64::from(today.batches),
            all: batches,
            spark: sparks.batches,
        },
        positions: Count {
            today: today.positions,
            all: positions,
            spark: sparks.positions,
        },
        nodes: Count {
            today: today.nodes,
            all: nodes,
            spark: sparks.nodes,
        },
        knps: Knps {
            last: state.last_knps_per_core,
            nnue: avg(day.nnue_sum, day.nnue_n),
            hce: avg(day.hce_sum, day.hce_n),
            spark: sparks.knps,
        },
        nnue: Nnue {
            mnps,
            spark: sparks.mnps,
        },
    }
}

pub fn live(state: &State, now: SystemTime, tz: Tz) -> Live {
    Live {
        totals: totals(state, now, tz),
        batches: state
            .history
            .batches
            .iter()
            .take(PAGE_BATCHES)
            .cloned()
            .collect(),
        problems: state.history.problem_groups(now),
    }
}

pub fn players(state: &State, now: SystemTime, tz: Tz) -> PlayersView {
    let h = &state.history;
    let today = h.sum_since(local_midnight(secs(now), tz.tz));
    let mut by_title: Vec<(String, u64)> = h
        .all
        .by_title
        .iter()
        .map(|(t, n)| (t.clone(), *n))
        .collect();
    let rank = |t: &str| {
        TITLE_ORDER
            .iter()
            .position(|x| *x == t)
            .unwrap_or(TITLE_ORDER.len())
    };
    by_title.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then_with(|| a.0.cmp(&b.0)));
    PlayersView {
        today: PlayerCounts {
            looked: u64::from(today.looked),
            titled: u64::from(today.titled),
            patron: u64::from(today.patron),
            bots: None,
        },
        all: PlayerCounts {
            looked: h.all.looked,
            titled: h.all.titled,
            patron: h.all.patron,
            bots: Some(h.all.bots),
        },
        by_title,
    }
}

pub fn lookup_part(state: &State, now: SystemTime, tz: Tz) -> LookupPart {
    LookupPart {
        lookup: serde_json::to_value(state.lookup_status()).expect("serializes"),
        players: players(state, now, tz),
        top_games: state.history.top_games(),
    }
}

pub fn charts(state: &State, now: SystemTime) -> Charts {
    Charts {
        positions: state.history.charts(now),
        variants: state.history.variants(now),
    }
}

/// One topic's payload, as JSON.
pub fn topic(state: &State, ctx: &StatusContext, topic: Topic, now: SystemTime, tz: Tz) -> Value {
    let value = match topic {
        Topic::Status => serde_json::to_value(state.status(now, ctx)),
        Topic::Live => serde_json::to_value(live(state, now, tz)),
        Topic::Lookup => serde_json::to_value(lookup_part(state, now, tz)),
        Topic::Charts => serde_json::to_value(charts(state, now)),
    };
    value.expect("view serializes")
}

/// `/api/history`: every part except the status, merged into one object.
pub fn history(state: &State, now: SystemTime, tz: Tz) -> Value {
    let mut out = serde_json::Map::new();
    for part in [
        serde_json::to_value(live(state, now, tz)),
        serde_json::to_value(lookup_part(state, now, tz)),
        serde_json::to_value(charts(state, now)),
    ] {
        if let Ok(Value::Object(map)) = part {
            out.extend(map);
        }
    }
    Value::Object(out)
}

/// Locks the shared state, recovering from a poisoned lock.
pub fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;
    use std::time::Duration;

    const T0: u64 = 1_795_219_200;

    fn at(s: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(s)
    }

    fn ctx() -> StatusContext {
        StatusContext {
            host: "h".into(),
            listen: "l".into(),
            stall_timeout: Duration::from_secs(900),
        }
    }

    #[test]
    fn history_has_every_store_key() {
        let mut s = State::new(at(T0));
        s.apply_line(
            &parse_line(
                "[    |            ] https://lichess.org/AbCd1234 finished (1500 knps/core)",
            ),
            at(T0 + 5),
        );
        s.tick(at(T0 + 10));
        let v = history(&s, at(T0 + 70), Tz::default());
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "batches",
                "lookup",
                "players",
                "positions",
                "problems",
                "topGames",
                "totals",
                "variants"
            ]
        );
        assert_eq!(v["totals"]["batches"]["today"], 1);
        assert_eq!(v["totals"]["knps"]["nnue"], 1500);
        assert_eq!(v["batches"][0]["id"], "AbCd1234");
        assert_eq!(v["batches"][0]["lookup"], "skipped");
        assert_eq!(v["players"]["byTitle"], serde_json::json!([]));
        assert!(v["positions"]["24h"]["t"].is_array());
    }

    #[test]
    fn today_follows_the_page_time_zone() {
        let mut s = State::new(at(T0 - DAY));
        // 23:30 UTC the day before T0.
        s.apply_line(
            &parse_line(
                "[    |            ] https://lichess.org/AbCd1234 finished (1500 knps/core)",
            ),
            at(T0 - 1800),
        );
        let now = at(T0 + 3600);
        assert_eq!(totals(&s, now, Tz { tz: 0 }).batches.today, 0);
        // UTC+2: local midnight was 22:00 UTC.
        assert_eq!(totals(&s, now, Tz { tz: -120 }).batches.today, 1);
    }

    #[test]
    fn titles_are_ordered_strongest_first() {
        let mut s = State::new(at(T0));
        for t in ["LM", "IM", "GM", "XX"] {
            s.history.all.by_title.insert(t.into(), 1);
        }
        let order: Vec<_> = players(&s, at(T0), Tz::default())
            .by_title
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert_eq!(order, ["GM", "IM", "LM", "XX"]);
    }

    #[test]
    fn every_topic_serializes() {
        let s = State::new(at(T0));
        for t in Topic::ALL {
            assert!(topic(&s, &ctx(), t, at(T0), Tz::default()).is_object());
        }
    }

    #[tokio::test]
    async fn changes_wake_subscribers_per_topic() {
        let changes = Changes::default();
        let mut rx = changes.subscribe();
        changes.bump(Topic::Charts);
        assert!(rx[Topic::Charts as usize].has_changed().unwrap());
        assert!(!rx[Topic::Status as usize].has_changed().unwrap());
        rx[Topic::Charts as usize].changed().await.unwrap();
    }
}
