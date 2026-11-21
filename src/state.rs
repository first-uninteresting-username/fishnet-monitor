//! The monitor's view of fishnet, built from parsed log lines, the stats
//! file and the child process. Shared as `Arc<Mutex<State>>` (std mutex,
//! never held across an `.await`).
//!
//! Every method takes `now` explicitly, so tests don't depend on the clock.

use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime},
};

use serde::Serialize;

use crate::{
    history::{self, Activity, History, LookupMark, ProblemLevel},
    parser::{Event, Level, LogLine},
};

/// What fishnet is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    /// Child spawned, `### Running` not seen yet.
    Starting,
    /// Working (after `### Running`, a batch, a progress line, or after an
    /// idle wait has passed).
    Running,
    /// After `Going idle for ...` until the next batch/progress line.
    Idle,
    /// After `### Stopping soon` or `><> Stopping now.`, or a signal was
    /// forwarded to the child.
    Stopping,
    /// The child has exited.
    Stopped,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Starting => "starting",
            RunState::Running => "running",
            RunState::Idle => "idle",
            RunState::Stopping => "stopping",
            RunState::Stopped => "stopped",
        }
    }
}

/// Where [`State::totals`] currently comes from. The stats file wins over the
/// summary line once it has been read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TotalsSource {
    None,
    Summary,
    StatsFile,
}

/// fishnet's all-time counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub batches: u64,
    pub positions: u64,
    pub nodes: u64,
}

/// Bucket upper bounds for the knps/core histogram.
pub const KNPS_BUCKETS: &[f64] = &[
    100.0, 250.0, 500.0, 750.0, 1000.0, 1500.0, 2000.0, 3000.0, 5000.0,
];

/// A Prometheus-style cumulative histogram over [`KNPS_BUCKETS`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Histogram {
    /// `counts[i]` = observations `<= KNPS_BUCKETS[i]` (cumulative).
    pub counts: Vec<u64>,
    pub sum: f64,
    pub count: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram {
            counts: vec![0; KNPS_BUCKETS.len()],
            sum: 0.0,
            count: 0,
        }
    }
}

impl Histogram {
    pub fn observe(&mut self, value: f64) {
        for (bound, n) in KNPS_BUCKETS.iter().zip(&mut self.counts) {
            if value <= *bound {
                *n += 1;
            }
        }
        self.sum += value;
        self.count += 1;
    }
}

/// Something a webhook should be sent for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub kind: AlertKind,
    /// The triggering log line, or a short description.
    pub message: String,
    /// [`RunState`] at the time of the alert, or `"fatal"`.
    pub state: String,
    pub time: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Fatal,
    Stall,
}

impl AlertKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AlertKind::Fatal => "fatal",
            AlertKind::Stall => "stall",
        }
    }
}

/// Result of the health check behind `/healthz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Health {
    pub healthy: bool,
    /// Empty when healthy. Any of `"not running"`, `"fatal"`, `"stalled"`.
    pub reasons: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub started_at: SystemTime,
    /// The child process is alive.
    pub child_running: bool,
    /// Exit code once the child has exited (`None` while running, or if it
    /// was killed by a signal).
    pub exit_code: Option<i32>,
    pub stopped_at: Option<SystemTime>,
    pub run_state: RunState,

    /// Set by [`crate::parser::Event::Fatal`] or a panic; never cleared.
    pub fatal: bool,
    /// The line that made it fatal (with its `E: ` prefix restored).
    pub fatal_line: Option<String>,
    pub fatal_at: Option<SystemTime>,
    /// The last `Rejected` reason, kept for context.
    pub rejected_reason: Option<String>,

    pub version: Option<String>,
    pub endpoint: Option<String>,
    pub cores: Option<u32>,
    /// From the progress line (needs `-v`).
    pub queue_pending: Option<u32>,
    pub nnue_knps_per_core: Option<u32>,

    pub totals: Totals,
    pub totals_source: TotalsSource,

    /// Finished batches by `(variant, eval)` label values, e.g.
    /// `("standard", "nnue")`.
    pub batches_finished: BTreeMap<(String, String), u64>,
    pub knps_histogram: Histogram,
    pub last_knps_per_core: Option<u32>,
    pub last_batch_at: Option<SystemTime>,
    /// Last batch or idle line; `started_at` until the first one. Drives
    /// the stall check.
    pub last_activity_at: SystemTime,
    /// `now + wait` from the last idle line.
    pub idle_until: Option<SystemTime>,
    /// Batches still running after `### Stopping soon` (unknown → 0).
    pub stopping_batches: u32,

    /// Lines per level (`Level::as_str`).
    pub log_lines: BTreeMap<&'static str, u64>,
    pub unparsed_lines: u64,
    pub engine_errors: u64,
    pub engine_timeouts: u64,
    pub batches_aborted: u64,

    /// A stall alert has fired and not yet been re-armed by activity.
    pub stall_alerted: bool,

    /// What the dashboard shows over time; saved to `HISTORY_FILE`.
    pub history: History,
}

impl State {
    pub fn new(now: SystemTime) -> State {
        State {
            started_at: now,
            child_running: false,
            exit_code: None,
            stopped_at: None,
            run_state: RunState::Starting,
            fatal: false,
            fatal_line: None,
            fatal_at: None,
            rejected_reason: None,
            version: None,
            endpoint: None,
            cores: None,
            queue_pending: None,
            nnue_knps_per_core: None,
            totals: Totals::default(),
            totals_source: TotalsSource::None,
            batches_finished: BTreeMap::new(),
            knps_histogram: Histogram::default(),
            last_knps_per_core: None,
            last_batch_at: None,
            last_activity_at: now,
            idle_until: None,
            stopping_batches: 0,
            log_lines: BTreeMap::new(),
            unparsed_lines: 0,
            engine_errors: 0,
            engine_timeouts: 0,
            batches_aborted: 0,
            stall_alerted: false,
            history: History::new(now),
        }
    }

    /// Updates the state from one parsed line. Returns an alert when the
    /// line makes fishnet fatal for the first time (`Event::Fatal` or
    /// `Event::Panic`).
    pub fn apply_line(&mut self, line: &LogLine, now: SystemTime) -> Option<Alert> {
        *self.log_lines.entry(line.level.as_str()).or_insert(0) += 1;
        if line.is_unparsed() {
            self.unparsed_lines += 1;
        }

        if !matches!(line.event, Event::Fatal | Event::Panic) {
            let lvl = match line.level {
                Level::Warning => Some(ProblemLevel::W),
                Level::Error => Some(ProblemLevel::E),
                _ => None,
            };
            if let Some(lvl) = lvl {
                self.history.record_problem(now, lvl, &line.message);
            }
        }

        match &line.event {
            Event::BatchFinished(b) => {
                self.history.record_batch(now, b, LookupMark::Skipped);
                let key = (b.variant.as_str().to_string(), b.eval.as_str().to_string());
                *self.batches_finished.entry(key).or_insert(0) += 1;
                if let Some(knps) = b.knps_per_core {
                    self.knps_histogram.observe(f64::from(knps));
                    self.last_knps_per_core = Some(knps);
                }
                self.last_batch_at = Some(now);
                self.mark_activity(now);
                self.move_to(RunState::Running, &ACTIVE_STATES);
            }
            Event::Summary(s) => {
                self.version = Some(s.version.clone());
                self.nnue_knps_per_core = Some(s.nnue_knps_per_core);
                if let Some(cores) = self.cores {
                    let mnps = f64::from(s.nnue_knps_per_core) * f64::from(cores) / 1000.0;
                    self.history.record_speed(now, mnps);
                }
                // The stats file is more precise and is re-read on every
                // change, so it keeps priority over the summary.
                if self.totals_source != TotalsSource::StatsFile {
                    let totals = Totals {
                        batches: s.batches,
                        positions: s.positions,
                        nodes: s.nodes,
                    };
                    if self.totals_source == TotalsSource::Summary {
                        self.record_work(totals, now);
                    }
                    self.totals = totals;
                    self.totals_source = TotalsSource::Summary;
                }
            }
            Event::Progress(p) => {
                self.cores = Some(p.cores);
                self.queue_pending = Some(p.queued);
                self.move_to(RunState::Running, &[RunState::Starting, RunState::Idle]);
            }
            Event::Idle { wait } => {
                // mark_activity clears idle_until, so set it after.
                self.mark_activity(now);
                self.idle_until = now.checked_add(*wait);
                self.move_to(RunState::Idle, &ACTIVE_STATES);
            }
            Event::Fatal | Event::Panic => return self.become_fatal(line, now),
            Event::Rejected { reason } => self.rejected_reason = Some(reason.clone()),
            Event::Running => {
                self.move_to(RunState::Running, &[RunState::Starting, RunState::Idle]);
            }
            Event::StoppingSoon | Event::StoppingNow => {
                self.move_to(RunState::Stopping, &NOT_STOPPED_STATES);
            }
            Event::Version(v) => self.version = Some(v.clone()),
            Event::Endpoint(e) => self.endpoint = Some(e.clone()),
            Event::Cores(n) => self.cores = Some(*n),
            Event::BatchAborted { .. } => self.batches_aborted += 1,
            Event::EngineError => self.engine_errors += 1,
            Event::EngineTimeout { .. } => self.engine_timeouts += 1,
            _ => {}
        }
        None
    }

    /// The stats file was (re)read. The first reading is only a baseline;
    /// later ones add what changed to the history.
    pub fn apply_stats_file(&mut self, totals: Totals, now: SystemTime) {
        if self.totals_source == TotalsSource::StatsFile {
            self.record_work(totals, now);
        }
        self.totals = totals;
        self.totals_source = TotalsSource::StatsFile;
    }

    /// Adds the work between `self.totals` and `totals` to the history. A
    /// counter that went down (a new stats file) adds nothing.
    fn record_work(&mut self, totals: Totals, now: SystemTime) {
        if totals.positions >= self.totals.positions && totals.nodes >= self.totals.nodes {
            self.history.record_work(
                now,
                totals.positions - self.totals.positions,
                totals.nodes - self.totals.nodes,
            );
        }
    }

    /// What fishnet is doing right now, for the history's time accounting.
    pub fn activity(&self, now: SystemTime) -> Activity {
        if !self.child_running || self.run_state == RunState::Stopped {
            return Activity::Stopped;
        }
        if self.run_state == RunState::Idle {
            return Activity::Idle;
        }
        let quiet_since = match self.idle_until {
            Some(until) if until > self.last_activity_at => until,
            _ => self.last_activity_at,
        };
        let quiet = matches!(self.run_state, RunState::Starting | RunState::Running)
            && now
                .duration_since(quiet_since)
                .is_ok_and(|d| d >= history::QUIET_AFTER);
        if quiet {
            Activity::Quiet
        } else {
            Activity::Busy
        }
    }

    /// The `lookup` part of the page. The game lookup isn't built yet.
    pub fn lookup_status(&self) -> serde_json::Value {
        serde_json::json!({ "enabled": false, "state": "ok", "lastAt": null, "lastCount": 0,
            "queued": 0, "missingAll": self.history.all.missing, "retryAt": null })
    }

    /// Called every few seconds: time accounting and expiry in the
    /// history. Returns true when a new minute began.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let activity = self.activity(now);
        self.history.tick(now, activity)
    }

    /// The child was spawned.
    pub fn child_started(&mut self, now: SystemTime) {
        self.child_running = true;
        self.run_state = RunState::Starting;
        self.exit_code = None;
        self.stopped_at = None;
        self.last_activity_at = now;
        self.stall_alerted = false;
    }

    /// The monitor forwarded SIGINT/SIGTERM to the child.
    pub fn stop_requested(&mut self) {
        self.move_to(RunState::Stopping, &NOT_STOPPED_STATES);
    }

    /// The child exited. `code` is `None` when killed by a signal. Returns a
    /// fatal alert if it exited with an error (code != 0, or killed by a
    /// signal the monitor didn't forward) and no fatal alert was sent yet.
    pub fn child_exited(
        &mut self,
        code: Option<i32>,
        requested: bool,
        now: SystemTime,
    ) -> Option<Alert> {
        self.child_running = false;
        self.exit_code = code;
        self.stopped_at = Some(now);
        self.run_state = RunState::Stopped;
        self.queue_pending = None;

        // A signal we forwarded ourselves is a normal shutdown, whatever
        // code fishnet then exits with.
        let error_exit = !requested && code != Some(0);
        if !error_exit || self.fatal {
            return None;
        }
        let message = match code {
            Some(n) => format!("fishnet exited with code {n}"),
            None => "fishnet was killed by a signal".to_string(),
        };
        Some(Alert {
            kind: AlertKind::Fatal,
            message,
            state: "stopped".into(),
            time: now,
        })
    }

    /// Called periodically. Returns a stall alert once per stall.
    pub fn check_stall(&mut self, now: SystemTime, timeout: Duration) -> Option<Alert> {
        if self.stall_alerted || !self.is_stalled(now, timeout) {
            return None;
        }
        self.stall_alerted = true;
        // Whole seconds only, so the message has no sub-second part.
        let silent = now
            .duration_since(self.last_activity_at)
            .unwrap_or_default();
        let silent = Duration::from_secs(silent.as_secs());
        Some(Alert {
            kind: AlertKind::Stall,
            message: format!(
                "no batch and no idle line for {}",
                humantime::format_duration(silent)
            ),
            state: self.run_state.as_str().into(),
            time: now,
        })
    }

    /// Whether fishnet is stalled at `now`.
    pub fn is_stalled(&self, now: SystemTime, timeout: Duration) -> bool {
        if !self.child_running
            || self.fatal
            || matches!(self.run_state, RunState::Stopping | RunState::Stopped)
        {
            return false;
        }
        // An idle wait is announced in advance, so the stall clock starts
        // when that wait ends, not when the idle line was printed.
        let base = match self.idle_until {
            Some(until) if until > self.last_activity_at => until,
            _ => self.last_activity_at,
        };
        // An overflowing deadline is far in the future, so not stalled.
        base.checked_add(timeout)
            .is_some_and(|deadline| now > deadline)
    }

    pub fn health(&self, now: SystemTime, timeout: Duration) -> Health {
        let mut reasons = Vec::new();
        if !self.child_running {
            reasons.push("not running");
        }
        if self.fatal {
            reasons.push("fatal");
        }
        if self.is_stalled(now, timeout) {
            reasons.push("stalled");
        }
        Health {
            healthy: reasons.is_empty(),
            reasons,
        }
    }

    /// The JSON body of `/api/status`.
    pub fn status(&self, now: SystemTime, ctx: &StatusContext) -> Status {
        let health = self.health(now, ctx.stall_timeout);
        Status {
            host: ctx.host.clone(),
            listen: ctx.listen.clone(),
            state: if self.fatal {
                "fatal".into()
            } else {
                self.run_state.as_str().into()
            },
            healthy: health.healthy,
            health_reasons: health.reasons,
            version: self.version.clone(),
            endpoint: self.endpoint.clone(),
            cores: self.cores,
            queue: self.queue_pending,
            started_at: epoch_ms(self.started_at),
            last_batch_at: self.last_batch_at.map(epoch_ms),
            last_activity_at: epoch_ms(self.last_activity_at),
            idle_until: self.idle_until.map(epoch_ms),
            stopping_batches: self.stopping_batches,
            stopped_at: self.stopped_at.map(epoch_ms),
            exit_code: self.exit_code,
            fatal: match (&self.fatal_line, self.fatal_at) {
                (Some(line), Some(at)) => Some(FatalInfo {
                    line: line.clone(),
                    at: epoch_ms(at),
                }),
                _ => None,
            },
            stall_timeout: ctx.stall_timeout.as_secs(),
            stalled: self.is_stalled(now, ctx.stall_timeout),
            totals: self.totals,
            totals_source: self.totals_source,
            knps: KnpsInfo {
                last: self.last_knps_per_core,
                nnue: self.nnue_knps_per_core,
            },
        }
    }

    /// A batch or idle line: fishnet is working or waiting on purpose.
    /// Re-arms the stall alert.
    fn mark_activity(&mut self, now: SystemTime) {
        self.last_activity_at = now;
        self.idle_until = None;
        self.stall_alerted = false;
    }

    /// Changes the run state only if it is currently one of `allowed`.
    fn move_to(&mut self, to: RunState, allowed: &[RunState]) {
        if allowed.contains(&self.run_state) {
            self.run_state = to;
        }
    }

    fn become_fatal(&mut self, line: &LogLine, now: SystemTime) -> Option<Alert> {
        if self.fatal {
            return None;
        }
        // The parser strips the level prefix; put it back so the stored
        // line reads the way fishnet printed it.
        let prefix = match line.level {
            Level::Error => "E: ",
            Level::Warning => "W: ",
            Level::Debug => "D: ",
            Level::Status => "><> ",
            Level::Info => "",
        };
        let text = format!("{prefix}{}", line.message);
        self.history
            .record_problem(now, ProblemLevel::F, &line.message);
        self.fatal = true;
        self.fatal_at = Some(now);
        self.fatal_line = Some(text.clone());
        Some(Alert {
            kind: AlertKind::Fatal,
            message: text,
            state: "fatal".into(),
            time: now,
        })
    }
}

/// States in which fishnet is doing (or waiting to do) work.
const ACTIVE_STATES: [RunState; 3] = [RunState::Starting, RunState::Running, RunState::Idle];

/// Every state except the terminal `Stopped`.
const NOT_STOPPED_STATES: [RunState; 4] = [
    RunState::Starting,
    RunState::Running,
    RunState::Idle,
    RunState::Stopping,
];

/// Config values `/api/status` reports alongside the state.
#[derive(Debug, Clone)]
pub struct StatusContext {
    pub host: String,
    pub listen: String,
    pub stall_timeout: Duration,
}

/// `/api/status`. Field names match `client` in `frontend/src/store.js`;
/// times are epoch milliseconds (`u64`), durations seconds.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub host: String,
    pub listen: String,
    /// `RunState::as_str`, or `"fatal"` when `fatal` is set.
    pub state: String,
    pub healthy: bool,
    pub health_reasons: Vec<&'static str>,
    pub version: Option<String>,
    pub endpoint: Option<String>,
    pub cores: Option<u32>,
    pub queue: Option<u32>,
    pub started_at: u64,
    pub last_batch_at: Option<u64>,
    pub last_activity_at: u64,
    pub idle_until: Option<u64>,
    pub stopping_batches: u32,
    pub stopped_at: Option<u64>,
    pub exit_code: Option<i32>,
    pub fatal: Option<FatalInfo>,
    pub stall_timeout: u64,
    pub stalled: bool,
    pub totals: Totals,
    pub totals_source: TotalsSource,
    pub knps: KnpsInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FatalInfo {
    pub line: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnpsInfo {
    /// knps/core of the last finished batch.
    pub last: Option<u32>,
    /// NNUE knps/core from the summary line.
    pub nnue: Option<u32>,
}

/// Epoch milliseconds; 0 for times before the epoch.
pub fn epoch_ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_line;
    use serde_json::json;

    const BATCH: &str =
        "[    |            ] https://lichess.org/AbCd1234 finished (1234 knps/core)";
    const BATCH_ATOMIC_HCE_NO_SPEED: &str =
        "[    |            ] https://lichess.org/AbCd1234 finished (atomic, hce, ? nps)";
    const IDLE_60: &str = "Going idle for 60s.";
    const IDLE_300: &str = "Going idle for 300s.";
    const SUMMARY: &str = "><> v2.15.1: 1180 knps/core (nnue), 1.234 batches, 76.543 positions, 98.765.432.100 total nodes";
    const PROGRESS: &str =
        "[==  |            ] 4 cores, 2 queued, latest: https://lichess.org/AbCd1234#17";
    const RUNNING: &str = "### Running (SIGINT to stop) ...";
    const STOPPING_SOON: &str = "### Stopping soon. SIGINT again to abort pending batches ...";
    const STOPPING_NOW: &str = "><> Stopping now.";
    const FATAL: &str = "E: Client update or reconfiguration might be required. Stopping queue.";
    const PANIC: &str = "thread 'main' panicked at src/configure.rs:407:18:";
    const REJECTED: &str = "E: Server rejected request: Invalid or revoked key";
    const BANNER_VERSION: &str =
        r"#        / /\_   \ /      |     |_|   |_|___/_| |_|_| \_|\___|\__| 2.15.1";
    const ENDPOINT: &str = "Endpoint: https://lichess.org/fishnet";
    const CORES: &str = "Cores: 4";
    const ABORTED: &str = "W: Aborting batch 9e4f01ab.";
    const ENGINE_ERROR: &str = "E: Engine error: Broken pipe (os error 32)";
    const ENGINE_TIMEOUT: &str =
        "W: Official Stockfish timed out in worker 2. Context: https://lichess.org/Xk3pQ9aB#12";

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn feed(state: &mut State, raw: &str, now: SystemTime) -> Option<Alert> {
        state.apply_line(&parse_line(raw), now)
    }

    fn ctx() -> StatusContext {
        StatusContext {
            host: "monitor-host".into(),
            listen: "127.0.0.1:8080".into(),
            stall_timeout: Duration::from_secs(60),
        }
    }

    fn fatal_alert(message: &str, at: u64) -> Alert {
        Alert {
            kind: AlertKind::Fatal,
            message: message.into(),
            state: "fatal".into(),
            time: t(at),
        }
    }

    #[test]
    fn history_gets_batches_problems_and_fatal_lines() {
        let mut s = State::new(t(1000));
        feed(&mut s, BATCH, t(1001));
        feed(&mut s, ABORTED, t(1002));
        feed(&mut s, ENGINE_ERROR, t(1003));
        feed(&mut s, "D: some debug detail", t(1003));
        feed(&mut s, FATAL, t(1004));

        assert_eq!(s.history.batches.len(), 1);
        assert_eq!(s.history.minutes[0].batches, 1);
        let lines: Vec<_> = s
            .history
            .problems
            .iter()
            .map(|p| (p.lvl, p.msg.as_str()))
            .collect();
        assert_eq!(
            lines,
            vec![
                (
                    ProblemLevel::F,
                    "Client update or reconfiguration might be required. Stopping queue."
                ),
                (ProblemLevel::E, "Engine error: Broken pipe (os error 32)"),
                (ProblemLevel::W, "Aborting batch 9e4f01ab."),
            ]
        );
    }

    #[test]
    fn stats_file_changes_become_work_in_the_history() {
        let mut s = State::new(t(1000));
        let totals = |positions, nodes| Totals {
            batches: 1,
            positions,
            nodes,
        };
        // The first reading is the baseline.
        s.apply_stats_file(totals(100, 1000), t(1001));
        assert_eq!(s.history.all.positions, 0);
        s.apply_stats_file(totals(160, 1600), t(1002));
        assert_eq!((s.history.all.positions, s.history.all.nodes), (60, 600));
        // A reset file adds nothing.
        s.apply_stats_file(totals(5, 50), t(1003));
        assert_eq!(s.history.all.positions, 60);
        // The summary doesn't count once the stats file is read.
        feed(&mut s, SUMMARY, t(1004));
        assert_eq!(s.history.all.positions, 60);
    }

    #[test]
    fn summary_changes_become_work_without_a_stats_file() {
        let mut s = State::new(t(1000));
        feed(&mut s, CORES, t(1000));
        feed(&mut s, SUMMARY, t(1000));
        assert_eq!(s.history.all.positions, 0, "first summary is the baseline");
        assert_eq!(s.history.minutes[0].mnps, Some(1180.0 * 4.0 / 1000.0));
        s.totals.positions -= 40;
        s.totals.nodes -= 400;
        feed(&mut s, SUMMARY, t(1120));
        assert_eq!((s.history.all.positions, s.history.all.nodes), (40, 400));
    }

    #[test]
    fn activity_for_time_accounting() {
        let mut s = State::new(t(1000));
        assert_eq!(s.activity(t(1000)), Activity::Stopped);
        s.child_started(t(1000));
        assert_eq!(s.activity(t(1010)), Activity::Busy);
        assert_eq!(s.activity(t(1180)), Activity::Quiet);
        feed(&mut s, "Going idle for 90s.", t(1200));
        assert_eq!(s.activity(t(1250)), Activity::Idle);
        feed(&mut s, BATCH, t(1300));
        // Quiet counts from the end of the idle wait.
        assert_eq!(s.activity(t(1400)), Activity::Busy);
        assert_eq!(s.activity(t(1500)), Activity::Quiet);
        s.child_exited(Some(0), true, t(1600));
        assert_eq!(s.activity(t(1600)), Activity::Stopped);
        assert!(s.tick(t(1600)));
    }

    #[test]
    fn counts_levels_and_unparsed_lines() {
        let mut s = State::new(t(1000));
        feed(&mut s, "something newer fishnet prints", t(1001)); // info, unparsed
        feed(&mut s, "W: Unexpected status for queue status", t(1001)); // warning, not unparsed
        feed(&mut s, "D: some debug detail", t(1001)); // debug, not unparsed
        feed(&mut s, CORES, t(1001)); // info, parsed

        assert_eq!(s.log_lines["info"], 2);
        assert_eq!(s.log_lines["warning"], 1);
        assert_eq!(s.log_lines["debug"], 1);
        assert_eq!(s.unparsed_lines, 1);
    }

    #[test]
    fn batch_finished_counts_histogram_and_activity() {
        let mut s = State::new(t(1000));
        assert_eq!(feed(&mut s, BATCH, t(1100)), None);

        let key = ("standard".to_string(), "nnue".to_string());
        assert_eq!(s.batches_finished[&key], 1);
        assert_eq!(s.last_knps_per_core, Some(1234));
        assert_eq!(s.knps_histogram.count, 1);
        assert_eq!(s.last_batch_at, Some(t(1100)));
        assert_eq!(s.last_activity_at, t(1100));
        assert_eq!(s.run_state, RunState::Running);

        // No speed: counted per variant/eval, but the speed fields stay put.
        feed(&mut s, BATCH_ATOMIC_HCE_NO_SPEED, t(1200));
        let key = ("atomic".to_string(), "hce".to_string());
        assert_eq!(s.batches_finished[&key], 1);
        assert_eq!(s.knps_histogram.count, 1);
        assert_eq!(s.last_knps_per_core, Some(1234));
        assert_eq!(s.last_batch_at, Some(t(1200)));
    }

    #[test]
    fn summary_sets_fields_and_totals() {
        let mut s = State::new(t(1000));
        feed(&mut s, SUMMARY, t(1000));

        assert_eq!(s.version.as_deref(), Some("2.15.1"));
        assert_eq!(s.nnue_knps_per_core, Some(1180));
        assert_eq!(
            s.totals,
            Totals {
                batches: 1234,
                positions: 76_543,
                nodes: 98_765_432_100,
            }
        );
        assert_eq!(s.totals_source, TotalsSource::Summary);
    }

    #[test]
    fn stats_file_wins_over_summary_in_either_order() {
        let file = Totals {
            batches: 5,
            positions: 6,
            nodes: 7,
        };

        let mut s = State::new(t(1000));
        s.apply_stats_file(file, t(1000));
        feed(&mut s, SUMMARY, t(1000));
        assert_eq!(s.totals, file);
        assert_eq!(s.totals_source, TotalsSource::StatsFile);

        // A summary that arrives first is replaced by the stats file.
        let mut s = State::new(t(1000));
        feed(&mut s, SUMMARY, t(1000));
        s.apply_stats_file(file, t(1000));
        assert_eq!(s.totals, file);
        assert_eq!(s.totals_source, TotalsSource::StatsFile);
    }

    #[test]
    fn progress_starts_running_but_is_not_activity() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert_eq!(feed(&mut s, PROGRESS, t(1010)), None);

        assert_eq!(s.cores, Some(4));
        assert_eq!(s.queue_pending, Some(2));
        assert_eq!(s.run_state, RunState::Running);
        assert_eq!(s.last_activity_at, t(1000));
    }

    #[test]
    fn idle_sets_deadline_and_batch_clears_it() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, RUNNING, t(1000));
        feed(&mut s, IDLE_60, t(1100));

        assert_eq!(s.idle_until, Some(t(1160)));
        assert_eq!(s.last_activity_at, t(1100));
        assert_eq!(s.run_state, RunState::Idle);

        feed(&mut s, BATCH, t(1120));
        assert_eq!(s.idle_until, None);
        assert_eq!(s.run_state, RunState::Running);
    }

    #[test]
    fn running_line_leaves_running_unchanged_and_resumes_from_idle() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, IDLE_60, t(1000));
        assert_eq!(s.run_state, RunState::Idle);

        feed(&mut s, RUNNING, t(1001));
        assert_eq!(s.run_state, RunState::Running);
        // Not activity, so the stall clock is untouched.
        assert_eq!(s.last_activity_at, t(1000));
    }

    #[test]
    fn stopping_lines_hold_until_child_exits() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, RUNNING, t(1001));
        feed(&mut s, STOPPING_SOON, t(1002));
        assert_eq!(s.run_state, RunState::Stopping);

        // Batches and idle lines still count as activity, but do not
        // move a stopping fishnet back to running or idle.
        feed(&mut s, BATCH, t(1003));
        assert_eq!(s.last_batch_at, Some(t(1003)));
        assert_eq!(s.run_state, RunState::Stopping);
        feed(&mut s, IDLE_60, t(1004));
        assert_eq!(s.run_state, RunState::Stopping);
        feed(&mut s, PROGRESS, t(1005));
        assert_eq!(s.run_state, RunState::Stopping);
        feed(&mut s, STOPPING_NOW, t(1006));
        assert_eq!(s.run_state, RunState::Stopping);

        assert_eq!(s.child_exited(Some(0), true, t(1007)), None);
        assert_eq!(s.run_state, RunState::Stopped);
        feed(&mut s, BATCH, t(1008));
        assert_eq!(s.run_state, RunState::Stopped);
    }

    #[test]
    fn stop_requested_moves_to_stopping_unless_stopped() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.stop_requested();
        assert_eq!(s.run_state, RunState::Stopping);

        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.child_exited(Some(0), false, t(1001));
        s.stop_requested();
        assert_eq!(s.run_state, RunState::Stopped);
    }

    #[test]
    fn fatal_alerts_once_with_the_restored_prefix() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert_eq!(
            feed(&mut s, FATAL, t(1200)),
            Some(fatal_alert(
                "E: Client update or reconfiguration might be required. Stopping queue.",
                1200
            ))
        );
        assert!(s.fatal);
        assert_eq!(s.fatal_at, Some(t(1200)));
        assert_eq!(
            s.fatal_line.as_deref(),
            Some("E: Client update or reconfiguration might be required. Stopping queue.")
        );

        // A second fatal line, or the panic that follows, sends nothing and
        // leaves the first record alone.
        assert_eq!(feed(&mut s, FATAL, t(1210)), None);
        assert_eq!(feed(&mut s, PANIC, t(1211)), None);
        assert_eq!(s.fatal_at, Some(t(1200)));
    }

    #[test]
    fn panic_is_fatal_without_prefix() {
        let mut s = State::new(t(1000));
        assert_eq!(
            feed(&mut s, PANIC, t(1001)),
            Some(fatal_alert(
                "thread 'main' panicked at src/configure.rs:407:18:",
                1001
            ))
        );
        assert_eq!(
            s.fatal_line.as_deref(),
            Some("thread 'main' panicked at src/configure.rs:407:18:")
        );
    }

    #[test]
    fn rejection_is_recorded_without_alert() {
        let mut s = State::new(t(1000));
        assert_eq!(feed(&mut s, REJECTED, t(1001)), None);
        assert_eq!(s.rejected_reason.as_deref(), Some("Invalid or revoked key"));
        assert!(!s.fatal);
    }

    #[test]
    fn version_endpoint_and_cores_lines() {
        let mut s = State::new(t(1000));
        feed(&mut s, BANNER_VERSION, t(1000));
        feed(&mut s, ENDPOINT, t(1000));
        feed(&mut s, CORES, t(1000));

        assert_eq!(s.version.as_deref(), Some("2.15.1"));
        assert_eq!(s.endpoint.as_deref(), Some("https://lichess.org/fishnet"));
        assert_eq!(s.cores, Some(4));
    }

    #[test]
    fn engine_and_abort_counters() {
        let mut s = State::new(t(1000));
        assert_eq!(feed(&mut s, ABORTED, t(1001)), None);
        assert_eq!(feed(&mut s, ENGINE_ERROR, t(1001)), None);
        assert_eq!(feed(&mut s, ENGINE_TIMEOUT, t(1001)), None);

        assert_eq!(s.batches_aborted, 1);
        assert_eq!(s.engine_errors, 1);
        assert_eq!(s.engine_timeouts, 1);
        // Problems are not activity.
        assert_eq!(s.last_activity_at, t(1000));
    }

    #[test]
    fn child_started_resets_the_previous_run() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.child_exited(Some(3), false, t(1100));
        assert_eq!(s.exit_code, Some(3));

        s.child_started(t(2000));
        assert!(s.child_running);
        assert_eq!(s.run_state, RunState::Starting);
        assert_eq!(s.exit_code, None);
        assert_eq!(s.stopped_at, None);
        assert_eq!(s.last_activity_at, t(2000));
    }

    #[test]
    fn clean_exit_has_no_alert() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, PROGRESS, t(1001));
        assert_eq!(s.child_exited(Some(0), false, t(1100)), None);

        assert!(!s.child_running);
        assert_eq!(s.exit_code, Some(0));
        assert_eq!(s.stopped_at, Some(t(1100)));
        assert_eq!(s.run_state, RunState::Stopped);
        assert_eq!(s.queue_pending, None);
        assert!(!s.fatal);
    }

    #[test]
    fn error_exit_alerts_without_setting_fatal() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert_eq!(
            s.child_exited(Some(1), false, t(1100)),
            Some(Alert {
                kind: AlertKind::Fatal,
                message: "fishnet exited with code 1".into(),
                state: "stopped".into(),
                time: t(1100),
            })
        );
        assert!(!s.fatal);
    }

    #[test]
    fn signal_kill_alerts_unless_we_forwarded_it() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert_eq!(
            s.child_exited(None, false, t(1100)).map(|a| a.message),
            Some("fishnet was killed by a signal".to_string())
        );

        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.stop_requested();
        assert_eq!(s.child_exited(None, true, t(1100)), None);
        assert_eq!(s.exit_code, None);
    }

    #[test]
    fn requested_exit_gives_no_alert_even_with_error_code() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.stop_requested();
        assert_eq!(s.child_exited(Some(1), true, t(1100)), None);
        assert_eq!(s.run_state, RunState::Stopped);
        assert!(!s.is_stalled(t(9999), Duration::from_secs(60)));
    }

    #[test]
    fn error_exit_after_fatal_sends_no_second_alert() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert!(feed(&mut s, FATAL, t(1200)).is_some());
        assert_eq!(s.child_exited(Some(1), false, t(1201)), None);
    }

    #[test]
    fn stall_fires_once_and_rearms_after_a_batch() {
        let timeout = Duration::from_secs(60);
        let mut s = State::new(t(1000));
        s.child_started(t(1000));

        // Deadline is strictly after last activity + timeout.
        assert_eq!(s.check_stall(t(1060), timeout), None);
        assert_eq!(
            s.check_stall(t(1061), timeout),
            Some(Alert {
                kind: AlertKind::Stall,
                message: "no batch and no idle line for 1m 1s".into(),
                state: "starting".into(),
                time: t(1061),
            })
        );
        // Still stalled, but already alerted.
        assert_eq!(s.check_stall(t(1100), timeout), None);
        assert!(s.stall_alerted);

        feed(&mut s, BATCH, t(1100));
        assert!(!s.stall_alerted);
        assert_eq!(s.check_stall(t(1150), timeout), None);
        let alert = s.check_stall(t(1161), timeout).expect("re-armed");
        assert_eq!(alert.kind, AlertKind::Stall);
        assert_eq!(alert.message, "no batch and no idle line for 1m 1s");
        assert_eq!(alert.state, "running");
    }

    #[test]
    fn stall_rearms_after_an_idle_line() {
        let timeout = Duration::from_secs(60);
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        assert!(s.check_stall(t(1061), timeout).is_some());

        feed(&mut s, IDLE_60, t(1100));
        assert!(!s.stall_alerted);
    }

    #[test]
    fn idle_wait_extends_the_stall_deadline() {
        let timeout = Duration::from_secs(60);
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, RUNNING, t(1000));
        feed(&mut s, IDLE_300, t(1000));
        assert_eq!(s.idle_until, Some(t(1300)));

        // Deadline is idle_until + timeout = 1360, not 1060.
        assert!(!s.is_stalled(t(1350), timeout));
        assert_eq!(s.check_stall(t(1350), timeout), None);
        assert!(s.is_stalled(t(1361), timeout));
        let alert = s.check_stall(t(1361), timeout).expect("stall");
        assert_eq!(alert.message, "no batch and no idle line for 6m 1s");
        assert_eq!(alert.state, "idle");
    }

    #[test]
    fn no_stall_when_not_running_stopping_or_fatal() {
        let timeout = Duration::from_secs(60);

        // Never started.
        let s = State::new(t(1000));
        assert!(!s.is_stalled(t(5000), timeout));

        // Stopping.
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.stop_requested();
        assert!(!s.is_stalled(t(5000), timeout));

        // Fatal.
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, FATAL, t(1001));
        assert!(!s.is_stalled(t(5000), timeout));

        // Exited.
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        s.child_exited(Some(0), false, t(1001));
        assert!(!s.is_stalled(t(5000), timeout));
    }

    #[test]
    fn health_reasons_come_in_order() {
        let timeout = Duration::from_secs(60);

        let s = State::new(t(1000));
        let h = s.health(t(1000), timeout);
        assert!(!h.healthy);
        assert_eq!(h.reasons, vec!["not running"]);

        let mut s = State::new(t(1000));
        feed(&mut s, FATAL, t(1001));
        let h = s.health(t(1001), timeout);
        assert_eq!(h.reasons, vec!["not running", "fatal"]);

        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        let h = s.health(t(1100), timeout);
        assert_eq!(h.reasons, vec!["stalled"]);
        assert!(!h.healthy);

        s.child_started(t(1100));
        feed(&mut s, BATCH, t(1101));
        let h = s.health(t(1102), timeout);
        assert!(h.healthy);
        assert!(h.reasons.is_empty());
    }

    #[test]
    fn status_json_uses_camel_case_and_epoch_millis() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, BATCH, t(1100));
        feed(&mut s, SUMMARY, t(1100));
        feed(&mut s, PROGRESS, t(1110));

        let status = s.status(t(1120), &ctx());
        assert_eq!(status.host, "monitor-host");
        assert_eq!(status.state, "running");
        assert_eq!(status.stall_timeout, 60);
        assert_eq!(status.queue, Some(2));
        assert_eq!(status.fatal, None);

        let v = serde_json::to_value(&status).unwrap();
        assert_eq!(v["state"], "running");
        assert_eq!(v["healthy"], true);
        assert_eq!(v["healthReasons"], json!([]));
        assert_eq!(v["startedAt"], 1_000_000);
        assert_eq!(v["lastBatchAt"], 1_100_000);
        assert_eq!(v["lastActivityAt"], 1_100_000);
        assert!(v["idleUntil"].is_null());
        assert!(v["stoppedAt"].is_null());
        assert!(v["exitCode"].is_null());
        assert!(v["fatal"].is_null());
        assert_eq!(v["stallTimeout"], 60);
        assert_eq!(v["stalled"], false);
        assert_eq!(v["queue"], 2);
        assert_eq!(v["cores"], 4);
        assert_eq!(v["version"], "2.15.1");
        assert_eq!(v["totalsSource"], "summary");
        assert_eq!(v["totals"]["positions"], 76_543);
        assert_eq!(v["knps"]["last"], 1234);
        assert_eq!(v["knps"]["nnue"], 1180);
        assert_eq!(v["listen"], "127.0.0.1:8080");
    }

    #[test]
    fn status_reports_fatal_and_its_line() {
        let mut s = State::new(t(1000));
        s.child_started(t(1000));
        feed(&mut s, FATAL, t(1200));

        let v = serde_json::to_value(s.status(t(1210), &ctx())).unwrap();
        assert_eq!(v["state"], "fatal");
        assert_eq!(v["healthy"], false);
        assert_eq!(v["healthReasons"], json!(["fatal"]));
        assert_eq!(
            v["fatal"]["line"],
            "E: Client update or reconfiguration might be required. Stopping queue."
        );
        assert_eq!(v["fatal"]["at"], 1_200_000);
    }

    #[test]
    fn status_before_start_has_no_batch_time_and_status_is_starting() {
        let s = State::new(t(1000));
        let v = serde_json::to_value(s.status(t(1000), &ctx())).unwrap();
        assert_eq!(v["state"], "starting");
        assert!(v["lastBatchAt"].is_null());
        assert_eq!(v["knps"]["last"], serde_json::Value::Null);
        assert_eq!(v["totalsSource"], "none");
    }
}
