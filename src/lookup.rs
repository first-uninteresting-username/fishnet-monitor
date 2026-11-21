//! Game lookup (see "Game lookup" in `PLAN.md`). fishnet's work has a game
//! id but no players, so the monitor asks the public lichess API for them:
//! `POST <LICHESS_API>/api/games/export/_ids`, up to 300 ids per request,
//! NDJSON back, no auth.
//!
//! - [`Queue`] lives in [`State`] and holds the ids of finished batches whose
//!   URL is on the `LICHESS_API` host.
//! - It sends every 60 s, or as soon as 300 ids wait. One request at a time.
//! - On a 429 it waits a full minute, as the API docs ask. A failed request
//!   puts its ids back; the queue is capped, so an outage can't grow it
//!   without limit (the oldest ids are dropped).
//! - [`Source::Fixture`] answers from an NDJSON file instead of the network,
//!   for `fishnet-monitor demo` and tests.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    history::{Game, LookupMark, Player, ProblemLevel, millis},
    parser::GameUrl,
    state::State,
    view::{self, Changes, Topic},
};

/// Ids per request (the API's limit).
pub const BATCH: usize = 300;
/// How long ids wait for more to join them, and the minimum gap between
/// requests.
pub const INTERVAL: Duration = Duration::from_secs(60);
/// The wait after a 429 or a failed request.
pub const BACKOFF: Duration = Duration::from_secs(60);
/// Most ids kept waiting.
pub const MAX_QUEUE: usize = 3000;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LookupState {
    Ok,
    Ratelimited,
    Error,
}

/// Request outcomes since the monitor started, for `/metrics`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    /// Games found.
    pub ok: u64,
    /// Games lichess didn't return.
    pub missing: u64,
    /// Failed requests.
    pub error: u64,
    /// Requests answered with 429.
    pub ratelimited: u64,
    /// Ids dropped because the queue was full.
    pub dropped: u64,
}

/// An id waiting for the lookup, with when its batch finished (epoch ms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    pub at: u64,
}

#[derive(Debug, Clone)]
pub struct Queue {
    pub enabled: bool,
    /// `scheme://host[:port]` of `LICHESS_API`; only games on it are looked up.
    pub origin: String,
    pending: VecDeque<Pending>,
    /// When the first id of the current wait was queued.
    waiting_since: Option<SystemTime>,
    last_sent: Option<SystemTime>,
    retry_at: Option<SystemTime>,
    in_flight: bool,
    pub state: LookupState,
    last_at: Option<SystemTime>,
    last_count: usize,
    pub counts: Counts,
}

/// The `lookup` signal in `store.js`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub state: LookupState,
    pub last_at: Option<u64>,
    pub last_count: usize,
    pub queued: usize,
    pub missing_all: u64,
    pub retry_at: Option<u64>,
}

/// `scheme://host[:port]` of a base URL such as `https://lichess.org/`.
pub fn origin_of(base: &str) -> String {
    let base = base.trim_end_matches('/');
    match base.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.split('/').next().unwrap_or_default();
            format!("{scheme}://{host}")
        }
        None => base.to_owned(),
    }
}

impl Queue {
    pub fn new(enabled: bool, lichess_api: &str) -> Queue {
        Queue {
            enabled,
            origin: origin_of(lichess_api),
            pending: VecDeque::new(),
            waiting_since: None,
            last_sent: None,
            retry_at: None,
            in_flight: false,
            state: LookupState::Ok,
            last_at: None,
            last_count: 0,
            counts: Counts::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Queues a finished batch's game. Returns how its batch is marked and
    /// the ids dropped to make room.
    pub fn offer(&mut self, game: &GameUrl, now: SystemTime) -> (LookupMark, Vec<String>) {
        if !self.enabled || game.origin != self.origin {
            return (LookupMark::Skipped, Vec::new());
        }
        if !self.pending.iter().any(|p| p.id == game.game_id) {
            if self.pending.is_empty() {
                self.waiting_since = Some(now);
            }
            self.pending.push_back(Pending {
                id: game.game_id.clone(),
                at: millis(now),
            });
        }
        (LookupMark::Pending, self.trim())
    }

    /// Drops the oldest ids beyond [`MAX_QUEUE`].
    fn trim(&mut self) -> Vec<String> {
        let mut dropped = Vec::new();
        while self.pending.len() > MAX_QUEUE {
            if let Some(p) = self.pending.pop_front() {
                dropped.push(p.id);
            }
        }
        self.counts.dropped += dropped.len() as u64;
        dropped
    }

    /// Whether a request should go out at `now`.
    pub fn due(&self, now: SystemTime) -> bool {
        if !self.enabled || self.in_flight || self.pending.is_empty() {
            return false;
        }
        if self.retry_at.is_some_and(|t| now < t) {
            return false;
        }
        if self.pending.len() >= BATCH {
            return true;
        }
        let waited = |since: Option<SystemTime>| {
            since.is_none_or(|t| now.duration_since(t).is_ok_and(|d| d >= INTERVAL))
        };
        waited(self.waiting_since) && waited(self.last_sent)
    }

    /// Takes the next request's ids if one is due, and marks it in flight.
    pub fn take_due(&mut self, now: SystemTime) -> Option<Vec<Pending>> {
        if !self.due(now) {
            return None;
        }
        let n = self.pending.len().min(BATCH);
        let ids: Vec<Pending> = self.pending.drain(..n).collect();
        self.waiting_since = (!self.pending.is_empty()).then_some(now);
        self.last_sent = Some(now);
        self.in_flight = true;
        Some(ids)
    }

    /// Puts a failed request's ids back in front and waits [`BACKOFF`].
    /// Returns ids dropped because the queue overflowed.
    fn retry_later(&mut self, ids: Vec<Pending>, now: SystemTime) -> Vec<String> {
        for p in ids.into_iter().rev() {
            self.pending.push_front(p);
        }
        self.waiting_since = Some(now);
        self.retry_at = now.checked_add(BACKOFF);
        self.trim()
    }

    pub fn status(&self, missing_all: u64) -> Status {
        Status {
            enabled: self.enabled,
            state: self.state,
            last_at: self.last_at.map(millis),
            last_count: self.last_count,
            queued: self.pending.len(),
            missing_all,
            retry_at: self.retry_at.map(millis),
        }
    }
}

// ---------- the export API ----------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiGame {
    pub id: String,
    #[serde(default)]
    pub rated: bool,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub speed: Option<String>,
    pub players: ApiPlayers,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiPlayers {
    pub white: ApiPlayer,
    pub black: ApiPlayer,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayer {
    #[serde(default)]
    pub user: Option<ApiUser>,
    #[serde(default)]
    pub rating: Option<u32>,
    #[serde(default)]
    pub ai_level: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiUser {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Set for active Patrons (the API docs' sign of a supporter).
    #[serde(default)]
    pub patron_color: Option<serde_json::Value>,
}

impl ApiPlayer {
    fn player(&self) -> Player {
        if let Some(ai) = self.ai_level {
            return Player::Ai { ai };
        }
        let user = self.user.as_ref();
        Player::Human {
            name: user.map_or_else(|| "Anonymous".to_owned(), |u| u.name.clone()),
            rating: self.rating,
            title: user.and_then(|u| u.title.clone()),
            patron: user.is_some_and(|u| u.patron_color.as_ref().is_some_and(|c| !c.is_null())),
        }
    }
}

impl ApiGame {
    /// The game as the history keeps it; `at` is when its batch finished.
    pub fn game(&self, at: u64) -> Game {
        let white = self.players.white.player();
        let black = self.players.black.player();
        Game {
            id: self.id.clone(),
            at,
            rating: Game::average_rating(&white, &black),
            white,
            black,
            speed: self.speed.clone().unwrap_or_else(|| "unknown".to_owned()),
            variant: self.variant.clone().filter(|v| v != "standard"),
            rated: self.rated,
        }
    }
}

/// Parses an NDJSON export. Lines that don't parse are skipped (their games
/// then count as missing).
pub fn parse_ndjson(body: &str) -> Vec<ApiGame> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    RateLimited,
    Failed(String),
}

/// Where games come from.
pub enum Source {
    /// The lichess API at `base` (`LICHESS_API`).
    Http {
        client: reqwest::Client,
        base: String,
    },
    /// Games by id from an NDJSON file, no network.
    Fixture(HashMap<String, ApiGame>),
}

impl Source {
    pub fn http(base: &str) -> Source {
        Source::Http {
            client: reqwest::Client::new(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    pub fn fixture(ndjson: &str) -> Source {
        Source::Fixture(
            parse_ndjson(ndjson)
                .into_iter()
                .map(|g| (g.id.clone(), g))
                .collect(),
        )
    }

    pub async fn fetch(&self, ids: &[String]) -> Result<Vec<ApiGame>, FetchError> {
        match self {
            Source::Fixture(games) => {
                Ok(ids.iter().filter_map(|id| games.get(id).cloned()).collect())
            }
            Source::Http { client, base } => {
                let url = format!(
                    "{base}/api/games/export/_ids?moves=false&tags=false&clocks=false&evals=false&opening=false"
                );
                let response = client
                    .post(url)
                    .header(reqwest::header::ACCEPT, "application/x-ndjson")
                    .header(reqwest::header::CONTENT_TYPE, "text/plain")
                    .body(ids.join(","))
                    .timeout(TIMEOUT)
                    .send()
                    .await
                    .map_err(|e| FetchError::Failed(e.without_url().to_string()))?;
                let status = response.status();
                if status.as_u16() == 429 {
                    return Err(FetchError::RateLimited);
                }
                if !status.is_success() {
                    return Err(FetchError::Failed(format!("HTTP {}", status.as_u16())));
                }
                let body = response
                    .text()
                    .await
                    .map_err(|e| FetchError::Failed(e.without_url().to_string()))?;
                Ok(parse_ndjson(&body))
            }
        }
    }
}

impl State {
    /// Applies a finished request to the queue and the history.
    pub fn finish_lookup(
        &mut self,
        now: SystemTime,
        ids: Vec<Pending>,
        result: Result<Vec<ApiGame>, FetchError>,
    ) {
        self.lookup.in_flight = false;
        let dropped = match result {
            Ok(games) => {
                self.lookup.state = LookupState::Ok;
                self.lookup.retry_at = None;
                self.lookup.last_at = Some(now);
                self.lookup.last_count = ids.len();
                let mut by_id: HashMap<String, ApiGame> =
                    games.into_iter().map(|g| (g.id.clone(), g)).collect();
                for p in ids {
                    match by_id.remove(&p.id) {
                        Some(api) => {
                            self.lookup.counts.ok += 1;
                            self.history.record_game(now, api.game(p.at));
                        }
                        None => {
                            self.lookup.counts.missing += 1;
                            self.history.record_missing(&p.id);
                        }
                    }
                }
                Vec::new()
            }
            Err(FetchError::RateLimited) => {
                self.lookup.state = LookupState::Ratelimited;
                self.lookup.counts.ratelimited += 1;
                self.lookup.retry_later(ids, now)
            }
            Err(FetchError::Failed(err)) => {
                self.lookup.state = LookupState::Error;
                self.lookup.counts.error += 1;
                let msg = format!("fishnet-monitor: game lookup failed: {err}");
                self.history.record_problem(now, ProblemLevel::W, &msg);
                self.lookup.retry_later(ids, now)
            }
        };
        for id in dropped {
            self.history.mark_batches(&id, LookupMark::Skipped, None);
        }
    }
}

/// One step of [`run`] at `now`: sends a request if one is due. Returns
/// whether one was sent.
pub async fn step(
    state: &Mutex<State>,
    changes: &Changes,
    source: &Source,
    now: SystemTime,
) -> bool {
    let Some(ids) = view::lock(state).lookup.take_due(now) else {
        return false;
    };
    changes.bump(Topic::Lookup);
    let keys: Vec<String> = ids.iter().map(|p| p.id.clone()).collect();
    let result = source.fetch(&keys).await;
    view::lock(state).finish_lookup(now, ids, result);
    changes.bump_all(&[Topic::Lookup, Topic::Live]);
    true
}

/// Checks the queue every second. Runs forever; spawn it as a task.
pub async fn run(
    state: std::sync::Arc<Mutex<State>>,
    changes: std::sync::Arc<Changes>,
    source: Source,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        step(&state, &changes, &source, SystemTime::now()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(s)
    }

    fn url(s: &str) -> GameUrl {
        GameUrl::parse(s).unwrap()
    }

    #[test]
    fn origin_drops_paths_and_slashes() {
        assert_eq!(origin_of("https://lichess.org/"), "https://lichess.org");
        assert_eq!(
            origin_of("http://localhost:9663/x"),
            "http://localhost:9663"
        );
    }

    #[test]
    fn only_games_on_the_api_host_are_queued() {
        let mut q = Queue::new(true, "https://lichess.org");
        let (mark, _) = q.offer(&url("https://lichess.org/AbCd1234"), at(0));
        assert_eq!(mark, LookupMark::Pending);
        let (mark, _) = q.offer(&url("http://localhost:9663/AbCd1234"), at(0));
        assert_eq!(mark, LookupMark::Skipped);
        // The same game twice waits once.
        q.offer(&url("https://lichess.org/AbCd1234"), at(1));
        assert_eq!(q.len(), 1);

        let mut off = Queue::new(false, "https://lichess.org");
        assert_eq!(
            off.offer(&url("https://lichess.org/AbCd1234"), at(0)).0,
            LookupMark::Skipped
        );
    }

    #[test]
    fn waits_60_s_unless_300_are_waiting() {
        let mut q = Queue::new(true, "https://lichess.org");
        q.offer(&url("https://lichess.org/a1"), at(100));
        assert!(!q.due(at(159)));
        q.offer(&url("https://lichess.org/a2"), at(150));
        assert!(q.due(at(160)), "60 s after the first id");
        let ids = q.take_due(at(160)).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(!q.due(at(500)), "one request at a time");
        q.in_flight = false;

        // Next request at least 60 s after the last one.
        q.offer(&url("https://lichess.org/b1"), at(170));
        assert!(!q.due(at(219)));
        assert!(q.due(at(230)));

        // 300 waiting: right away.
        let mut q = Queue::new(true, "https://lichess.org");
        for i in 0..BATCH {
            q.offer(&url(&format!("https://lichess.org/g{i}")), at(0));
        }
        assert!(q.due(at(0)));
        assert_eq!(q.take_due(at(0)).unwrap().len(), BATCH);
    }

    #[test]
    fn requests_take_at_most_300() {
        let mut q = Queue::new(true, "https://lichess.org");
        for i in 0..(BATCH + 5) {
            q.offer(&url(&format!("https://lichess.org/g{i}")), at(0));
        }
        let ids = q.take_due(at(0)).unwrap();
        assert_eq!(ids.len(), BATCH);
        assert_eq!(ids[0].id, "g0");
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn queue_is_capped_dropping_the_oldest() {
        let mut q = Queue::new(true, "https://lichess.org");
        let mut dropped = Vec::new();
        for i in 0..(MAX_QUEUE + 2) {
            dropped.extend(q.offer(&url(&format!("https://lichess.org/g{i}")), at(0)).1);
        }
        assert_eq!(q.len(), MAX_QUEUE);
        assert_eq!(dropped, ["g0", "g1"]);
        assert_eq!(q.counts.dropped, 2);
    }

    #[test]
    fn parses_players_ai_and_supporters() {
        let games = parse_ndjson(concat!(
            r#"{"id":"a","rated":true,"variant":"standard","speed":"blitz","players":{"white":{"user":{"name":"W","title":"GM","patronColor":3},"rating":2600},"black":{"user":{"name":"B"},"rating":2401}}}"#,
            "\n",
            "not json\n",
            r#"{"id":"b","rated":false,"variant":"atomic","speed":"rapid","players":{"white":{"aiLevel":5},"black":{"user":{"name":"x","patronColor":null},"rating":1500}}}"#,
            "\n",
        ));
        assert_eq!(games.len(), 2);
        let a = games[0].game(7);
        assert_eq!(a.rating, Some(2501));
        assert_eq!(a.variant, None);
        assert_eq!(a.white.title(), Some("GM"));
        assert!(a.white.patron());
        assert!(!a.black.patron());
        let b = games[1].game(7);
        assert_eq!(b.white, Player::Ai { ai: 5 });
        assert_eq!(b.rating, None);
        assert_eq!(b.variant.as_deref(), Some("atomic"));
        assert!(!b.black.patron(), "a null patronColor is no supporter");
    }
}
