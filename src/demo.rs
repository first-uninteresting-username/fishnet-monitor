//! `fishnet-monitor demo`: the real monitor and dashboard with a fake
//! fishnet, so the UI can be worked on without a key or network.
//!
//! - The history starts with 7 weeks of generated data (charts, all-time
//!   counts, top games), so every panel has something to show.
//! - Then it prints the start-up lines of `tests/fixtures/normal.log` and,
//!   every few seconds, a batch line for a game from
//!   `tests/fixtures/games.ndjson`, plus the 2-minute summary, an idle spell
//!   now and then and a warning from `tests/fixtures/problems.log`.
//! - Every line goes through the parser and [`State`] like in `run`.
//! - The game lookup answers from the same fixture file; no network calls.
//!
//! Nothing is written to disk (`HISTORY_FILE` is ignored).

use std::{
    process::ExitCode,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use crate::{
    config::Config,
    history::{DAY, HOUR, MINUTE, ProblemLevel, secs},
    http::{self, AppState},
    lookup::{self, Queue, Source},
    parser::{Event, Level, parse_line},
    state::{State, StatusContext, Totals},
    view::{Changes, Topic},
};

const STARTUP: &str = include_str!("../tests/fixtures/normal.log");
const PROBLEMS: &str = include_str!("../tests/fixtures/problems.log");
pub const GAMES: &str = include_str!("../tests/fixtures/games.ndjson");
/// The demo's lichess, so the fixture games pass the lookup's host check.
const API: &str = "https://lichess.org";

/// A small seeded generator (Park–Miller), so runs look alike.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0 * 16807 % 2_147_483_647;
        self.0 as f64 / 2_147_483_647.0
    }

    fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as f64) as usize).min(n.saturating_sub(1))
    }
}

/// fishnet's short variant names (`short_variant_name` upstream); standard
/// and chess960 have none.
fn short_variant(lichess: &str) -> Option<&'static str> {
    match lichess {
        "antichess" => Some("anti"),
        "atomic" => Some("atomic"),
        "crazyhouse" => Some("zh"),
        "horde" => Some("horde"),
        "kingOfTheHill" => Some("koth"),
        "racingKings" => Some("race"),
        "threeCheck" => Some("3check"),
        _ => None,
    }
}

/// `12408` → `12.408`, the way fishnet groups digits in the summary.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// The fixture games as `(id, lichess variant)`.
fn games() -> Vec<(String, String)> {
    GAMES
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|v| {
            (
                v["id"].as_str().unwrap_or_default().to_owned(),
                v["variant"].as_str().unwrap_or("standard").to_owned(),
            )
        })
        .collect()
}

/// A batch line for game `id` of `variant`.
fn batch_line(rng: &mut Rng, id: &str, variant: &str) -> String {
    let hce = rng.next() < 0.1;
    let knps = if hce {
        900 + rng.below(400)
    } else {
        1650 + rng.below(380)
    };
    let mut tags: Vec<String> = Vec::new();
    tags.extend(short_variant(variant).map(str::to_owned));
    if hce {
        tags.push("hce".into());
    }
    tags.push(format!("{knps} knps/core"));
    format!(
        "[    |            ] {API}/{id} finished ({})",
        tags.join(", ")
    )
}

/// Fills the history with 7 weeks of plausible data ending at `now`.
fn backfill(state: &mut State, rng: &mut Rng, now: SystemTime) {
    let now_s = secs(now);
    let start = now_s - 7 * 7 * DAY;
    let h = &mut state.history;
    h.all.since = start * 1000;

    // Every fixture game, analysed at some point in the last 7 weeks. The
    // top lists and title counts come from the real lookup code.
    let source = Source::fixture(GAMES);
    if let Source::Fixture(by_id) = &source {
        let mut ids: Vec<&String> = by_id.keys().collect();
        ids.sort();
        let mut dated: Vec<(u64, &String)> = ids
            .into_iter()
            .map(|id| ((rng.next().powi(2) * (7 * 7 * DAY) as f64) as u64, id))
            .collect();
        dated.sort_by_key(|d| std::cmp::Reverse(d.0));
        for (ago, id) in dated {
            let game = &by_id[id];
            let at = SystemTime::UNIX_EPOCH + Duration::from_secs(now_s - ago);
            h.record_game(at, game.game(crate::history::millis(at)));
        }
    }
    // record_game also counted the games in buckets; the buckets are
    // generated below instead.
    h.minutes.clear();
    h.hours.clear();
    // Positions per minute follow the time of day, with idle spells.
    let rate = |t: u64, rng: &mut Rng| {
        let day = (t % DAY) as f64 / DAY as f64;
        let wave = 0.75 + 0.25 * (day * std::f64::consts::TAU + 1.3).sin();
        if rng.next() < 0.06 {
            0.0
        } else {
            (52.0 + rng.next() * 28.0) * wave
        }
    };
    let mut t = start - start % HOUR;
    while t + HOUR <= now_s - now_s % HOUR {
        let per_minute = rate(t, rng);
        let positions = (per_minute * 60.0) as u64;
        let bucket = crate::history::Bucket {
            t,
            batches: (positions / 45) as u32,
            positions,
            nodes: positions * 2_300_000,
            idle: if positions == 0 { 3600 } else { 0 },
            ..Default::default()
        };
        h.all.batches += u64::from(bucket.batches);
        h.all.positions += bucket.positions;
        h.all.nodes += bucket.nodes;
        h.hours.push_back(bucket);
        t += HOUR;
    }
    let mut t = now_s - 7 * DAY;
    t -= t % MINUTE;
    let mut idle_left = 0;
    while t < now_s - now_s % MINUTE {
        if idle_left == 0 && rng.next() < 0.004 {
            idle_left = 5 + rng.below(40);
        }
        let mut bucket = crate::history::Bucket {
            t,
            ..Default::default()
        };
        if idle_left > 0 {
            idle_left -= 1;
            bucket.idle = 60;
        } else {
            let positions = rate(t, rng).max(20.0) as u64;
            let batches = 1 + (positions / 45) as u32;
            bucket.positions = positions;
            bucket.nodes = positions * 2_300_000;
            bucket.batches = batches;
            for _ in 0..batches {
                let hce = rng.next() < 0.1;
                let knps = if hce {
                    900 + rng.below(400)
                } else {
                    1650 + rng.below(380)
                } as u64;
                if hce {
                    bucket.hce_sum += knps;
                    bucket.hce_n += 1;
                } else {
                    bucket.nnue_sum += knps;
                    bucket.nnue_n += 1;
                }
                let variant = [
                    "standard:nnue",
                    "standard:nnue",
                    "standard:nnue",
                    "chess960:nnue",
                    "atomic:hce",
                    "crazyhouse:hce",
                ][rng.below(6)];
                *bucket.variants.entry(variant.to_owned()).or_insert(0) += 1;
            }
            bucket.mnps = Some(5.0 + rng.next() * 1.2);
        }
        h.minutes.push_back(bucket);
        t += MINUTE;
    }

    for (ago, lvl, msg) in [
        (
            139 * MINUTE,
            ProblemLevel::W,
            "Fairy-Stockfish timed out in worker 1. Context: https://lichess.org/Va2nT3iB#4",
        ),
        (
            277 * MINUTE,
            ProblemLevel::E,
            "Too many requests. Suspending requests for 60s.",
        ),
        (
            276 * MINUTE,
            ProblemLevel::E,
            "Too many requests. Suspending requests for 45s.",
        ),
    ] {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(now_s - ago);
        h.record_problem(at, lvl, msg);
    }
    h.problems
        .make_contiguous()
        .sort_by_key(|p| std::cmp::Reverse(p.at));

    state.totals = Totals {
        batches: 58_000 + h.all.batches,
        positions: 2_700_000 + h.all.positions,
        nodes: 6_000_000_000_000 + h.all.nodes,
    };
    state.totals_source = crate::state::TotalsSource::StatsFile;
}

/// Applies one fake fishnet line and tells the page.
fn feed(state: &Mutex<State>, changes: &Changes, raw: &str) {
    let line = parse_line(raw);
    let batch = matches!(line.event, Event::BatchFinished(_));
    let problem = matches!(line.level, Level::Warning | Level::Error);
    let now = SystemTime::now();
    let mut s = state.lock().unwrap();
    s.apply_line(&line, now);
    if batch {
        // The stats file would grow with each batch.
        let mut totals = s.totals;
        totals.batches += 1;
        totals.positions += 30
            + (now
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_millis() as u64
                % 40);
        totals.nodes += totals.positions % 1000 * 2_300_000;
        s.apply_stats_file(totals, now);
    }
    drop(s);
    changes.bump(Topic::Status);
    if batch {
        changes.bump_all(&[Topic::Live, Topic::Lookup]);
    } else if problem {
        changes.bump(Topic::Live);
    }
}

pub fn run(config: Config) -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("fishnet-monitor: can't start the runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(demo(config))
}

async fn demo(config: Config) -> ExitCode {
    let now = SystemTime::now();
    let mut rng = Rng(7);
    let mut state = State::new(now);
    backfill(&mut state, &mut rng, now);
    state.lookup = Queue::new(true, API);
    let state = Arc::new(Mutex::new(state));
    let changes = Arc::new(Changes::default());

    let app = AppState {
        state: Arc::clone(&state),
        ctx: Arc::new(StatusContext {
            host: "demo".into(),
            listen: config.listen.to_string(),
            stall_timeout: config.stall_timeout,
        }),
        changes: Arc::clone(&changes),
    };
    let listener = match tokio::net::TcpListener::bind(config.listen).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("fishnet-monitor: can't serve on {}: {err}", config.listen);
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "fishnet-monitor demo: dashboard on http://{}/ (fake fishnet, no network; Ctrl-C to stop)",
        listener.local_addr().unwrap_or(config.listen)
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, http::router(app)).await;
    });
    tokio::spawn(lookup::run(
        Arc::clone(&state),
        Arc::clone(&changes),
        Source::fixture(GAMES),
    ));

    {
        let state = Arc::clone(&state);
        let changes = Arc::clone(&changes);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                if state.lock().unwrap().tick(SystemTime::now()) {
                    changes.bump_all(&[Topic::Charts, Topic::Live, Topic::Lookup]);
                }
            }
        });
    }

    tokio::select! {
        () = fake_fishnet(&state, &changes, rng) => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    ExitCode::SUCCESS
}

/// The fake client: start-up lines, then batches forever.
async fn fake_fishnet(state: &Mutex<State>, changes: &Changes, mut rng: Rng) {
    state.lock().unwrap().child_started(SystemTime::now());
    for line in STARTUP.lines() {
        feed(state, changes, line);
        if line.starts_with("### Running") {
            break;
        }
    }
    let warnings: Vec<&str> = PROBLEMS
        .lines()
        .filter(|l| {
            (l.starts_with("W: ") || l.starts_with("E: "))
                && !l.contains("Stopping queue")
                && !l.contains("rejected")
        })
        .collect();
    let games = games();
    let mut since_summary = Duration::ZERO;
    let mut since_problem = Duration::ZERO;
    loop {
        let wait = Duration::from_millis(3000 + rng.below(5000) as u64);
        tokio::time::sleep(wait).await;
        since_summary += wait;
        since_problem += wait;

        if rng.next() < 0.01 {
            feed(state, changes, "Going idle for 60s.");
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        }
        let (id, variant) = &games[rng.below(games.len())];
        let line = batch_line(&mut rng, id, variant);
        feed(state, changes, &line);

        if since_summary >= Duration::from_secs(120) {
            since_summary = Duration::ZERO;
            let line = {
                let s = state.lock().unwrap();
                format!(
                    "><> v2.15.1: {} knps/core (nnue), {} batches, {} positions, {} total nodes",
                    1300 + rng.below(100),
                    grouped(s.totals.batches),
                    grouped(s.totals.positions),
                    grouped(s.totals.nodes)
                )
            };
            feed(state, changes, &line);
        }
        if since_problem >= Duration::from_secs(180) && !warnings.is_empty() {
            since_problem = Duration::ZERO;
            feed(state, changes, warnings[rng.below(warnings.len())]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Activity;

    #[test]
    fn generated_lines_parse() {
        let mut rng = Rng(3);
        for (id, variant) in games() {
            let line = batch_line(&mut rng, &id, &variant);
            let parsed = parse_line(&line);
            let Event::BatchFinished(b) = parsed.event else {
                panic!("{line} is not a batch line");
            };
            assert_eq!(b.target.game().unwrap().game_id, id);
            let expected = if variant == "chess960" {
                "standard"
            } else {
                &variant
            };
            assert_eq!(b.variant.as_str(), expected);
        }
        let summary = format!(
            "><> v2.15.1: 1351 knps/core (nnue), {} batches, {} positions, {} total nodes",
            grouped(12408),
            grouped(703_561),
            grouped(1_020_164_832_117)
        );
        assert!(
            matches!(parse_line(&summary).event, Event::Summary(_)),
            "{summary}"
        );
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1.000");
    }

    #[test]
    fn backfill_fills_every_panel() {
        let now = SystemTime::now();
        let mut state = State::new(now);
        backfill(&mut state, &mut Rng(7), now);
        let h = &state.history;
        assert_eq!(h.minutes.len() as u64, 7 * DAY / MINUTE);
        assert!(h.hours.len() as u64 >= 7 * 7 * 24 - 1);
        assert!(!h.all.top.is_empty());
        assert!(!h.all.by_title.is_empty());
        assert!(!h.variants(now).is_empty());
        assert!(!h.problem_groups(now).is_empty());
        assert!(h.charts(now)["7w"].v.iter().any(|v| *v > 0.0));
        assert_eq!(state.activity(now), Activity::Stopped);
    }
}
