//! Parser for fishnet's log output.
//!
//! fishnet has no metrics API, so everything the monitor knows comes from the
//! lines it prints. The format is not a stable interface: the parser never
//! fails, and a line it doesn't recognise becomes [`Event::Other`]. Info and
//! status lines that end up there are counted as unparsed (see
//! [`LogLine::is_unparsed`]), so format drift after a fishnet release shows up
//! in the metrics instead of going unnoticed.
//!
//! The formats come from the fishnet source (`logger.rs`, `queue.rs`,
//! `main.rs`, `api.rs`, `stockfish.rs`, `configure.rs`, `stats.rs`).

use std::{sync::LazyLock, time::Duration};

use regex::Regex;

/// The log level, taken from the line's prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// No prefix.
    Info,
    /// `><> `: fishnet's own status lines.
    Status,
    /// `W: `
    Warning,
    /// `E: `
    Error,
    /// `D: `, only printed with `-v`.
    Debug,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Status => "status",
            Level::Warning => "warning",
            Level::Error => "error",
            Level::Debug => "debug",
        }
    }
}

/// One parsed line.
#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    pub level: Level,
    /// The line without its level prefix.
    pub message: String,
    pub event: Event,
}

impl LogLine {
    /// A line that should have matched a parser but didn't. Warnings, errors
    /// and debug lines are free-form, so only info and status lines count.
    pub fn is_unparsed(&self) -> bool {
        matches!(self.event, Event::Other) && matches!(self.level, Level::Info | Level::Status)
    }
}

/// What a line tells us.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `[bar] <url|batch id> finished (<variant>, hce, N knps/core)`. Analysis
    /// batches are logged at info level, move batches (playing against
    /// lichess AI) only at debug level.
    BatchFinished(BatchFinished),
    /// `><> vX.Y.Z: N knps/core (nnue), N batches, N positions, N total nodes`,
    /// every 120 s.
    Summary(Summary),
    /// `[bar] N cores, N queued, latest: <url|batch id>`, only with `-v` when
    /// stdout is not a terminal.
    Progress(Progress),
    /// `Going idle for <duration>.` Info level for waits of 40 s or more,
    /// debug level for shorter ones.
    Idle { wait: Duration },
    /// `E: Client update or reconfiguration might be required. Stopping queue.`
    /// The server rejected the client; fishnet stops for good.
    Fatal,
    /// `E: Server rejected request: <reason>`, logged right before [`Event::Fatal`].
    Rejected { reason: String },
    /// `### Running (SIGINT to stop) ...`: configuration done, work starts.
    Running,
    /// `### Stopping soon. SIGINT again to abort pending batches ...`
    StoppingSoon,
    /// `><> Stopping now.`
    StoppingNow,
    /// The version at the end of the start-up banner.
    Version(String),
    /// `Endpoint: <url>`
    Endpoint(String),
    /// `Cores: N`
    Cores(u32),
    /// `Engines: <official>, <multi-variant> (for GPLv3, run: ... license)`
    Engines {
        official: String,
        multi_variant: String,
    },
    /// `CPU features: <flags>`
    CpuFeatures(String),
    /// `Backlog: Join queue if user backlog >= <d> or system backlog >= <d>`
    Backlog { user: Duration, system: Duration },
    /// `Resuming from "<path>" ...` or `Recording to new stats file "<path>" ...`
    StatsFile { path: String, resumed: bool },
    /// `W: Aborting batch <id>.`
    BatchAborted { batch_id: String },
    /// `E: Engine error: ...` or `E: Stockfish process N exited with status ...`
    EngineError,
    /// `W: Official Stockfish timed out in worker N. ...` (or Fairy-Stockfish)
    EngineTimeout { engine: Engine, worker: u32 },
    /// A Rust panic: `thread 'main' panicked at src/...`. fishnet exits.
    Panic,
    /// A known line that carries nothing we track: the banner, headlines,
    /// blank lines, update checks, engine restarts.
    Ignored,
    /// Anything else. See [`LogLine::is_unparsed`].
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Official Stockfish, used for standard chess.
    Official,
    /// Fairy-Stockfish, used for variants.
    MultiVariant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchFinished {
    pub target: BatchTarget,
    pub variant: Variant,
    pub eval: Eval,
    /// `None` when fishnet printed `? nps`.
    pub knps_per_core: Option<u32>,
    pub queue: QueueBar,
}

/// What a batch belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchTarget {
    /// A game, logged as `<endpoint host>/<game id>`.
    Game(GameUrl),
    /// A batch without a game, logged as `batch <id>`.
    Batch(String),
}

impl BatchTarget {
    pub fn game(&self) -> Option<&GameUrl> {
        match self {
            BatchTarget::Game(game) => Some(game),
            BatchTarget::Batch(_) => None,
        }
    }
}

/// A game URL as fishnet prints it: the endpoint with the path replaced by
/// the game id, so `https://lichess.org/fishnet` becomes
/// `https://lichess.org/AbCd1234`. Progress lines add the position as the
/// fragment (`#12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameUrl {
    pub url: String,
    /// `scheme://host[:port]`, to check against `LICHESS_API` before looking
    /// the game up.
    pub origin: String,
    pub game_id: String,
    pub position: Option<u32>,
}

impl GameUrl {
    pub fn parse(s: &str) -> Option<GameUrl> {
        let (scheme, rest) = s.split_once("://")?;
        if scheme.is_empty() || !scheme.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let (host, path) = rest.split_once('/')?;
        if host.is_empty() {
            return None;
        }
        let (path, fragment) = match path.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (path, None),
        };
        let path = path.split(['?']).next().unwrap_or_default();
        if path.is_empty() || !path.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(GameUrl {
            url: s.to_owned(),
            origin: format!("{scheme}://{host}"),
            game_id: path.to_owned(),
            position: fragment.and_then(|f| f.parse().ok()),
        })
    }
}

/// Chess variant, from fishnet's short names (`short_variant_name`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Variant {
    Standard,
    Antichess,
    Atomic,
    Crazyhouse,
    Horde,
    KingOfTheHill,
    RacingKings,
    ThreeCheck,
    /// A short name this parser doesn't know yet.
    Unknown(String),
}

impl Variant {
    fn from_short_name(name: &str) -> Variant {
        match name {
            "anti" => Variant::Antichess,
            "atomic" => Variant::Atomic,
            "zh" => Variant::Crazyhouse,
            "horde" => Variant::Horde,
            "koth" => Variant::KingOfTheHill,
            "race" => Variant::RacingKings,
            "3check" => Variant::ThreeCheck,
            other => Variant::Unknown(other.to_owned()),
        }
    }

    /// Stable label for metrics and the API (lichess's variant keys).
    pub fn as_str(&self) -> &str {
        match self {
            Variant::Standard => "standard",
            Variant::Antichess => "antichess",
            Variant::Atomic => "atomic",
            Variant::Crazyhouse => "crazyhouse",
            Variant::Horde => "horde",
            Variant::KingOfTheHill => "kingOfTheHill",
            Variant::RacingKings => "racingKings",
            Variant::ThreeCheck => "threeCheck",
            Variant::Unknown(name) => name,
        }
    }
}

/// Evaluation flavour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Eval {
    Nnue,
    /// Hand-crafted evaluation, marked `hce` in the line.
    Hce,
}

impl Eval {
    pub fn as_str(self) -> &'static str {
        match self {
            Eval::Nnue => "nnue",
            Eval::Hce => "hce",
        }
    }
}

/// The queue status bar at the start of batch and progress lines, for
/// example `[==  |            ]`. Everything left of `|` stands for the
/// cores, and `=` marks pending positions. It ends in `>` when the queue
/// overflows the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueBar {
    /// Bar cells left of the `|`.
    pub core_cells: u8,
    /// Filled (`=`) cells.
    pub filled_cells: u8,
    pub overflow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub version: String,
    /// fishnet's running NNUE speed estimate, in knps per core.
    pub nnue_knps_per_core: u32,
    /// How uncertain the estimate is: the number of `?` after it (0 to 3).
    pub uncertainty: u8,
    pub batches: u64,
    pub positions: u64,
    pub nodes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub cores: u32,
    pub queued: u32,
    pub latest: BatchTarget,
    pub queue: QueueBar,
}

macro_rules! regex {
    ($re:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($re).expect("valid regex"));
        &*RE
    }};
}

/// Parses one line of fishnet output. Never fails.
pub fn parse_line(raw: &str) -> LogLine {
    // A terminal progress line is redrawn with `\r`; keep what is visible.
    let line = raw.rsplit('\r').next().unwrap_or_default();
    let line = line.trim_end_matches(['\n', ' ', '\t']);

    let (level, message) = if let Some(rest) = line.strip_prefix("><> ") {
        (Level::Status, rest)
    } else if let Some(rest) = line.strip_prefix("W: ") {
        (Level::Warning, rest)
    } else if let Some(rest) = line.strip_prefix("E: ") {
        (Level::Error, rest)
    } else if let Some(rest) = line.strip_prefix("D: ") {
        (Level::Debug, rest)
    } else {
        (Level::Info, line)
    };

    LogLine {
        level,
        message: message.to_owned(),
        event: parse_message(level, message),
    }
}

fn parse_message(level: Level, msg: &str) -> Event {
    // Lines with a queue bar: batch finished and progress. Batch-finished
    // lines are info (analysis) or debug (moves).
    if let Some((queue, rest)) = split_queue_bar(msg) {
        if let Some(event) = parse_finished(queue, rest) {
            return event;
        }
        if let Some(event) = parse_progress(queue, rest) {
            return event;
        }
        return Event::Other;
    }

    if let Some(wait) = msg
        .strip_prefix("Going idle for ")
        .and_then(|rest| rest.strip_suffix('.'))
        .and_then(parse_debug_duration)
    {
        return Event::Idle { wait };
    }

    match level {
        Level::Status => parse_status(msg),
        Level::Warning => parse_warning(msg),
        Level::Error => parse_error(msg),
        Level::Info => parse_info(msg),
        Level::Debug => Event::Ignored,
    }
}

fn parse_status(msg: &str) -> Event {
    let re = regex!(
        r"^v(\S+): (\d+) knps/core( \?{1,3})? \(nnue\), ([\d.]+) batches, ([\d.]+) positions, ([\d.]+) total nodes$"
    );
    if let Some(c) = re.captures(msg) {
        let summary = (|| {
            Some(Summary {
                version: c[1].to_owned(),
                nnue_knps_per_core: c[2].parse().ok()?,
                uncertainty: c.get(3).map_or(0, |m| m.as_str().trim().len() as u8),
                batches: parse_dot_thousands(&c[4])?,
                positions: parse_dot_thousands(&c[5])?,
                nodes: parse_dot_thousands(&c[6])?,
            })
        })();
        return summary.map_or(Event::Other, Event::Summary);
    }
    if msg == "Stopping now." {
        return Event::StoppingNow;
    }
    if msg.starts_with("Fishnet v")
        || msg.starts_with("Checking for updates")
        || msg.starts_with("Downloading v")
    {
        return Event::Ignored;
    }
    Event::Other
}

fn parse_warning(msg: &str) -> Event {
    if let Some(id) = msg
        .strip_prefix("Aborting batch ")
        .and_then(|rest| rest.strip_suffix('.'))
    {
        return Event::BatchAborted {
            batch_id: id.to_owned(),
        };
    }
    let re = regex!(r"^(Official Stockfish|Fairy-Stockfish) timed out in worker (\d+)\.");
    if let Some(c) = re.captures(msg)
        && let Ok(worker) = c[2].parse()
    {
        let engine = if &c[1] == "Official Stockfish" {
            Engine::Official
        } else {
            Engine::MultiVariant
        };
        return Event::EngineTimeout { engine, worker };
    }
    Event::Other
}

fn parse_error(msg: &str) -> Event {
    if msg == "Client update or reconfiguration might be required. Stopping queue." {
        return Event::Fatal;
    }
    if let Some(reason) = msg.strip_prefix("Server rejected request: ") {
        return Event::Rejected {
            reason: reason.to_owned(),
        };
    }
    let re = regex!(r"^Stockfish process \d+ exited with status ");
    if msg.starts_with("Engine error: ") || re.is_match(msg) {
        return Event::EngineError;
    }
    Event::Other
}

fn parse_info(msg: &str) -> Event {
    if msg.is_empty() {
        return Event::Ignored;
    }
    if let Some(title) = msg.strip_prefix("### ") {
        return if title.starts_with("Running (") {
            Event::Running
        } else if title.starts_with("Stopping soon.") {
            Event::StoppingSoon
        } else {
            Event::Ignored
        };
    }
    // Start-up banner. The line with the "fishNet" lettering ends with the
    // version.
    if msg.starts_with('#') {
        let re = regex!(r"\|_\|\s.*\s(\d+\.\d+\.\d+\S*)$");
        return match re.captures(msg) {
            Some(c) => Event::Version(c[1].to_owned()),
            None => Event::Ignored,
        };
    }
    if let Some(url) = msg.strip_prefix("Endpoint: ") {
        return Event::Endpoint(url.to_owned());
    }
    if let Some(cores) = msg.strip_prefix("Cores: ").and_then(|n| n.parse().ok()) {
        return Event::Cores(cores);
    }
    if let Some(features) = msg.strip_prefix("CPU features: ") {
        return Event::CpuFeatures(features.to_owned());
    }
    let re = regex!(r"^Engines: ([^,]+), (\S+) \(for GPLv3, run: .* license\)$");
    if let Some(c) = re.captures(msg) {
        return Event::Engines {
            official: c[1].to_owned(),
            multi_variant: c[2].to_owned(),
        };
    }
    let re = regex!(r"^Backlog: Join queue if user backlog >= (\S+) or system backlog >= (\S+)$");
    if let Some(c) = re.captures(msg)
        && let (Some(user), Some(system)) =
            (parse_debug_duration(&c[1]), parse_debug_duration(&c[2]))
    {
        return Event::Backlog { user, system };
    }
    let re = regex!(r#"^(Resuming from|Recording to new stats file) "(.*)" \.\.\.$"#);
    if let Some(c) = re.captures(msg) {
        return Event::StatsFile {
            path: c[2].to_owned(),
            resumed: &c[1] == "Resuming from",
        };
    }
    if regex!(r"^thread '.*' panicked at ").is_match(msg) {
        return Event::Panic;
    }
    if regex!(r"^Waiting \S+ before attempting to start engine$").is_match(msg)
        || msg.starts_with("note: run with `RUST_BACKTRACE")
    {
        return Event::Ignored;
    }
    Event::Other
}

/// Splits `[bar] rest` into the parsed bar and `rest`.
fn split_queue_bar(msg: &str) -> Option<(QueueBar, &str)> {
    let inner = msg.strip_prefix('[')?;
    let end = inner.find([']', '>'])?;
    let (cells, after) = (&inner[..end], &inner[end + 1..]);
    let rest = after.strip_prefix(' ')?;
    let (left, right) = cells.split_once('|')?;
    if !cells.bytes().all(|b| matches!(b, b' ' | b'=' | b'|')) || right.contains('|') {
        return None;
    }
    let count = |s: &str| s.bytes().filter(|&b| b == b'=').count() as u8;
    Some((
        QueueBar {
            core_cells: left.len() as u8,
            filled_cells: count(left) + count(right),
            overflow: inner.as_bytes()[end] == b'>',
        },
        rest,
    ))
}

fn parse_finished(queue: QueueBar, rest: &str) -> Option<Event> {
    let re = regex!(r"^(?:batch (\S+)|(\S+)) finished \(([^)]*)\)$");
    let c = re.captures(rest)?;
    let target = match (c.get(1), c.get(2)) {
        (Some(id), _) => BatchTarget::Batch(id.as_str().to_owned()),
        (None, Some(url)) => BatchTarget::Game(GameUrl::parse(url.as_str())?),
        (None, None) => return None,
    };

    let mut variant = Variant::Standard;
    let mut eval = Eval::Nnue;
    let mut knps_per_core = None;
    let mut speed_seen = false;
    for part in c[3].split(", ") {
        if part == "hce" {
            eval = Eval::Hce;
        } else if part == "? nps" {
            speed_seen = true;
        } else if let Some(n) = part.strip_suffix(" knps/core") {
            knps_per_core = Some(n.parse().ok()?);
            speed_seen = true;
        } else if !part.is_empty() && !part.contains(' ') {
            variant = Variant::from_short_name(part);
        } else {
            return None;
        }
    }
    // fishnet always ends the list with the speed.
    if !speed_seen {
        return None;
    }

    Some(Event::BatchFinished(BatchFinished {
        target,
        variant,
        eval,
        knps_per_core,
        queue,
    }))
}

fn parse_progress(queue: QueueBar, rest: &str) -> Option<Event> {
    let re = regex!(r"^(\d+) cores, (\d+) queued, latest: (\S+)$");
    let c = re.captures(rest)?;
    let latest = &c[3];
    let latest = match GameUrl::parse(latest) {
        Some(game) => BatchTarget::Game(game),
        None => BatchTarget::Batch(latest.to_owned()),
    };
    Some(Event::Progress(Progress {
        cores: c[1].parse().ok()?,
        queued: c[2].parse().ok()?,
        latest,
        queue,
    }))
}

/// Parses `1.234.567` (fishnet's `dot_thousands`).
fn parse_dot_thousands(s: &str) -> Option<u64> {
    let mut groups = s.split('.');
    let first = groups.next()?;
    if first.is_empty() || first.len() > 3 {
        return None;
    }
    let mut n: u64 = first.parse().ok()?;
    for group in groups {
        if group.len() != 3 || !group.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        n = n.checked_mul(1000)?.checked_add(group.parse().ok()?)?;
    }
    Some(n)
}

/// Parses Rust's `Debug` output for `Duration`: `52.123456789s`, `60s`,
/// `1.5ms`, `250µs`, `0ns`.
fn parse_debug_duration(s: &str) -> Option<Duration> {
    // Longest suffix first: "ms" also ends in "s".
    let (number, unit_nanos) = [
        ("ns", 1.0),
        ("µs", 1e3),
        ("us", 1e3),
        ("ms", 1e6),
        ("s", 1e9),
    ]
    .into_iter()
    .find_map(|(unit, nanos)| Some((s.strip_suffix(unit)?, nanos)))?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    Duration::try_from_secs_f64(value * unit_nanos / 1e9).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(line: &str) -> Event {
        parse_line(line).event
    }

    fn finished(line: &str) -> BatchFinished {
        match event(line) {
            Event::BatchFinished(b) => b,
            other => panic!("expected BatchFinished for {line:?}, got {other:?}"),
        }
    }

    #[test]
    fn levels() {
        assert_eq!(parse_line("Cores: 4").level, Level::Info);
        assert_eq!(parse_line("><> Stopping now.").level, Level::Status);
        assert_eq!(parse_line("W: Aborting batch x.").level, Level::Warning);
        assert_eq!(parse_line("E: Engine error: boom").level, Level::Error);
        assert_eq!(parse_line("D: Queue actor started").level, Level::Debug);
        assert_eq!(
            parse_line("E: Engine error: boom").message,
            "Engine error: boom"
        );
    }

    #[test]
    fn batch_finished_standard() {
        let b =
            finished("[    |            ] https://lichess.org/AbCd1234 finished (1234 knps/core)");
        assert_eq!(b.variant, Variant::Standard);
        assert_eq!(b.eval, Eval::Nnue);
        assert_eq!(b.knps_per_core, Some(1234));
        let game = b.target.game().unwrap();
        assert_eq!(game.game_id, "AbCd1234");
        assert_eq!(game.origin, "https://lichess.org");
        assert_eq!(game.position, None);
        assert_eq!(
            b.queue,
            QueueBar {
                core_cells: 4,
                filled_cells: 0,
                overflow: false
            }
        );
    }

    #[test]
    fn batch_finished_variants() {
        for (short, variant) in [
            ("anti", Variant::Antichess),
            ("atomic", Variant::Atomic),
            ("zh", Variant::Crazyhouse),
            ("horde", Variant::Horde),
            ("koth", Variant::KingOfTheHill),
            ("race", Variant::RacingKings),
            ("3check", Variant::ThreeCheck),
        ] {
            let b = finished(&format!(
                "[==  |            ] https://lichess.org/xYz98765 finished ({short}, 800 knps/core)"
            ));
            assert_eq!(b.variant, variant);
            assert_eq!(b.eval, Eval::Nnue);
        }
        let b = finished(
            "[    |            ] https://lichess.org/xYz98765 finished (giveaway, 1 knps/core)",
        );
        assert_eq!(b.variant, Variant::Unknown("giveaway".into()));
        assert_eq!(b.variant.as_str(), "giveaway");
    }

    #[test]
    fn batch_finished_hce_and_unknown_speed() {
        let b = finished(
            "[====|===         ] https://lichess.org/AbCd1234 finished (hce, 2100 knps/core)",
        );
        assert_eq!((b.variant, b.eval), (Variant::Standard, Eval::Hce));
        let b = finished(
            "[====|            ] https://lichess.org/AbCd1234 finished (atomic, hce, ? nps)",
        );
        assert_eq!((b.variant, b.eval), (Variant::Atomic, Eval::Hce));
        assert_eq!(b.knps_per_core, None);
        let b = finished("[    |            ] https://lichess.org/AbCd1234 finished (? nps)");
        assert_eq!(b.knps_per_core, None);
    }

    #[test]
    fn batch_finished_without_game() {
        let b = finished("[    |            ] batch 6f1c2a9e finished (1500 knps/core)");
        assert_eq!(b.target, BatchTarget::Batch("6f1c2a9e".into()));
        assert!(b.target.game().is_none());
        // Move batches are only logged with -v.
        let line = parse_line("D: [=   |            ] batch mv0001 finished (? nps)");
        assert_eq!(line.level, Level::Debug);
        assert!(matches!(line.event, Event::BatchFinished(_)));
    }

    #[test]
    fn batch_finished_custom_endpoint() {
        let b =
            finished("[ |               ] http://localhost:9663/gAmE0001 finished (5 knps/core)");
        let game = b.target.game().unwrap();
        assert_eq!(game.origin, "http://localhost:9663");
        assert_eq!(game.game_id, "gAmE0001");
    }

    #[test]
    fn queue_bar_overflow() {
        let b =
            finished("[====|============> https://lichess.org/AbCd1234 finished (900 knps/core)");
        assert_eq!(
            b.queue,
            QueueBar {
                core_cells: 4,
                filled_cells: 16,
                overflow: true
            }
        );
    }

    #[test]
    fn malformed_batch_lines_are_other() {
        for line in [
            "[    |            ] https://lichess.org/AbCd1234 finished (fast blitz, 12 knps/core)",
            "[    |            ] https://lichess.org/AbCd1234 finished (atomic)",
            "[    |            ] https://lichess.org/AbCd1234 finished (x knps/core)",
            "[    |            ] https://lichess.org/AbCd1234 started (1 knps/core)",
            "[    |            ] not-a-url finished (1 knps/core)",
            "[abc] https://lichess.org/AbCd1234 finished (1 knps/core)",
        ] {
            assert_eq!(event(line), Event::Other, "{line}");
            assert!(parse_line(line).is_unparsed(), "{line}");
        }
    }

    #[test]
    fn summary() {
        let e = event(
            "><> v2.15.1: 1180 knps/core (nnue), 1.234 batches, 76.543 positions, 98.765.432.100 total nodes",
        );
        assert_eq!(
            e,
            Event::Summary(Summary {
                version: "2.15.1".into(),
                nnue_knps_per_core: 1180,
                uncertainty: 0,
                batches: 1234,
                positions: 76543,
                nodes: 98_765_432_100,
            })
        );
    }

    #[test]
    fn summary_uncertainty() {
        for (marks, n) in [("", 0), (" ?", 1), (" ??", 2), (" ???", 3)] {
            let line = format!(
                "><> v2.15.1: 400 knps/core{marks} (nnue), 0 batches, 0 positions, 0 total nodes"
            );
            match event(&line) {
                Event::Summary(s) => assert_eq!(s.uncertainty, n, "{line}"),
                other => panic!("{line}: {other:?}"),
            }
        }
    }

    #[test]
    fn summary_bad_numbers_is_other() {
        let line = "><> v2.15.1: 400 knps/core (nnue), 1.23 batches, 0 positions, 0 total nodes";
        assert_eq!(event(line), Event::Other);
    }

    #[test]
    fn progress() {
        let e =
            event("[==  |            ] 4 cores, 2 queued, latest: https://lichess.org/AbCd1234#17");
        let Event::Progress(p) = e else {
            panic!("{e:?}")
        };
        assert_eq!((p.cores, p.queued), (4, 2));
        assert_eq!(p.latest.game().unwrap().position, Some(17));
        assert_eq!(p.latest.game().unwrap().game_id, "AbCd1234");

        let e = event("[    |            ] 4 cores, 0 queued, latest: 6f1c2a9e#3");
        let Event::Progress(p) = e else {
            panic!("{e:?}")
        };
        assert_eq!(p.latest, BatchTarget::Batch("6f1c2a9e#3".into()));
    }

    #[test]
    fn terminal_progress_redraw() {
        let line = "\r[    |            ] 4 cores, 1 queued, latest: x   \r[=   |            ] 4 cores, 1 queued, latest: https://lichess.org/AbCd1234#2   ";
        let Event::Progress(p) = event(line) else {
            panic!()
        };
        assert_eq!(p.queue.filled_cells, 1);
    }

    #[test]
    fn idle() {
        assert_eq!(
            event("Going idle for 52.123456789s."),
            Event::Idle {
                wait: Duration::from_nanos(52_123_456_789)
            }
        );
        assert_eq!(
            event("D: Going idle for 1.5s."),
            Event::Idle {
                wait: Duration::from_millis(1500)
            }
        );
        assert_eq!(
            event("Going idle for 120s."),
            Event::Idle {
                wait: Duration::from_secs(120)
            }
        );
        assert_eq!(event("Going idle for a while."), Event::Other);
    }

    #[test]
    fn debug_durations() {
        assert_eq!(parse_debug_duration("0ns"), Some(Duration::ZERO));
        assert_eq!(
            parse_debug_duration("250µs"),
            Some(Duration::from_micros(250))
        );
        assert_eq!(
            parse_debug_duration("1.5ms"),
            Some(Duration::from_micros(1500))
        );
        assert_eq!(parse_debug_duration("5s"), Some(Duration::from_secs(5)));
        assert_eq!(parse_debug_duration("s"), None);
        assert_eq!(parse_debug_duration("-1s"), None);
        assert_eq!(parse_debug_duration("5m"), None);
    }

    #[test]
    fn dot_thousands() {
        assert_eq!(parse_dot_thousands("0"), Some(0));
        assert_eq!(parse_dot_thousands("12"), Some(12));
        assert_eq!(parse_dot_thousands("1.234.567"), Some(1_234_567));
        assert_eq!(parse_dot_thousands("1234"), None);
        assert_eq!(parse_dot_thousands("1.23"), None);
        assert_eq!(parse_dot_thousands(""), None);
    }

    #[test]
    fn fatal_and_rejection() {
        assert_eq!(
            event("E: Server rejected request: Invalid key"),
            Event::Rejected {
                reason: "Invalid key".into()
            }
        );
        assert_eq!(
            event("E: Client update or reconfiguration might be required. Stopping queue."),
            Event::Fatal
        );
        // Only as an error.
        assert_eq!(
            event("Client update or reconfiguration might be required. Stopping queue."),
            Event::Other
        );
    }

    #[test]
    fn lifecycle() {
        assert_eq!(event("### Running (SIGINT to stop) ..."), Event::Running);
        assert_eq!(event("### Running (CTRL-C to stop) ..."), Event::Running);
        assert_eq!(
            event("### Stopping soon. SIGINT again to abort pending batches ..."),
            Event::StoppingSoon
        );
        assert_eq!(event("><> Stopping now."), Event::StoppingNow);
        assert_eq!(event("### Checking configuration ..."), Event::Ignored);
        assert_eq!(event(""), Event::Ignored);
    }

    #[test]
    fn startup() {
        assert_eq!(
            event(r"#        / /\_   \ /      |     |_|   |_|___/_| |_|_| \_|\___|\__| 2.15.1"),
            Event::Version("2.15.1".into())
        );
        assert_eq!(event(r"#   _________         .    ."), Event::Ignored);
        assert_eq!(
            event("Endpoint: https://lichess.org/fishnet"),
            Event::Endpoint("https://lichess.org/fishnet".into())
        );
        assert_eq!(event("Cores: 16"), Event::Cores(16));
        assert_eq!(
            event("CPU features: SSE2 | POPCNT | SSE41 | AVX2 | FAST_BMI2"),
            Event::CpuFeatures("SSE2 | POPCNT | SSE41 | AVX2 | FAST_BMI2".into())
        );
        assert_eq!(
            event(
                "Engines: stockfish-x86-64-bmi2, fairy-stockfish-x86-64-bmi2 (for GPLv3, run: /fishnet license)"
            ),
            Event::Engines {
                official: "stockfish-x86-64-bmi2".into(),
                multi_variant: "fairy-stockfish-x86-64-bmi2".into(),
            }
        );
        assert_eq!(
            event("Backlog: Join queue if user backlog >= 0ns or system backlog >= 1800s"),
            Event::Backlog {
                user: Duration::ZERO,
                system: Duration::from_secs(1800)
            }
        );
        assert_eq!(
            event(r#"Resuming from "/data/fishnet-stats" ..."#),
            Event::StatsFile {
                path: "/data/fishnet-stats".into(),
                resumed: true
            }
        );
        assert_eq!(
            event(r#"Recording to new stats file "/data/fishnet-stats" ..."#),
            Event::StatsFile {
                path: "/data/fishnet-stats".into(),
                resumed: false
            }
        );
    }

    #[test]
    fn problems() {
        assert_eq!(
            event("W: Aborting batch 6f1c2a9e."),
            Event::BatchAborted {
                batch_id: "6f1c2a9e".into()
            }
        );
        assert_eq!(
            event(
                "W: Official Stockfish timed out in worker 3. If this happens frequently it is better to stop and defer to clients with better hardware. Context: batch 6f1c2a9e"
            ),
            Event::EngineTimeout {
                engine: Engine::Official,
                worker: 3
            }
        );
        assert_eq!(
            event("W: Fairy-Stockfish timed out in worker 0. Context: batch 6f1c2a9e"),
            Event::EngineTimeout {
                engine: Engine::MultiVariant,
                worker: 0
            }
        );
        assert_eq!(event("E: Engine error: broken pipe"), Event::EngineError);
        assert_eq!(
            event("E: Stockfish process 1234 exited with status signal: 9 (SIGKILL)"),
            Event::EngineError
        );
        assert_eq!(
            event("thread 'main' panicked at src/configure.rs:410:18:"),
            Event::Panic
        );
    }

    #[test]
    fn free_form_problems_are_not_unparsed() {
        for line in [
            "W: Unexpected status for queue status: 502 Bad Gateway",
            "E: error sending request -> connection refused. Backing off 4.2s.",
            "D: User wait: 0ns due to ...",
        ] {
            let parsed = parse_line(line);
            assert_eq!(
                parsed.event,
                if line.starts_with("D:") {
                    Event::Ignored
                } else {
                    Event::Other
                }
            );
            assert!(!parsed.is_unparsed(), "{line}");
        }
    }

    #[test]
    fn unknown_lines_are_unparsed() {
        for line in [
            "Something new in fishnet 3.0",
            "><> v3.0.0: 1 Mnps (nnue), 1 batches",
            "Cores: many",
        ] {
            assert!(parse_line(line).is_unparsed(), "{line}");
        }
    }

    #[test]
    fn game_urls() {
        assert!(GameUrl::parse("https://lichess.org/").is_none());
        assert!(GameUrl::parse("https://lichess.org/a/b").is_none());
        assert!(GameUrl::parse("lichess.org/AbCd1234").is_none());
        assert!(GameUrl::parse("https:///AbCd1234").is_none());
        let g = GameUrl::parse("https://lichess.org/AbCd1234#0").unwrap();
        assert_eq!(g.position, Some(0));
        assert_eq!(g.url, "https://lichess.org/AbCd1234#0");
    }
}
