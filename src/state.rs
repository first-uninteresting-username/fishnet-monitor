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

use crate::parser::LogLine;

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
}

impl State {
    pub fn new(now: SystemTime) -> State {
        let _ = now;
        todo!("fragment: state")
    }

    /// Updates the state from one parsed line. Returns an alert when the
    /// line makes fishnet fatal for the first time (`Event::Fatal` or
    /// `Event::Panic`).
    pub fn apply_line(&mut self, line: &LogLine, now: SystemTime) -> Option<Alert> {
        let _ = (line, now);
        todo!("fragment: state")
    }

    /// The stats file was (re)read.
    pub fn apply_stats_file(&mut self, totals: Totals) {
        let _ = totals;
        todo!("fragment: state")
    }

    /// The child was spawned.
    pub fn child_started(&mut self, now: SystemTime) {
        let _ = now;
        todo!("fragment: state")
    }

    /// The monitor forwarded SIGINT/SIGTERM to the child.
    pub fn stop_requested(&mut self) {
        todo!("fragment: state")
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
        let _ = (code, requested, now);
        todo!("fragment: state")
    }

    /// Called periodically. Returns a stall alert once per stall.
    pub fn check_stall(&mut self, now: SystemTime, timeout: Duration) -> Option<Alert> {
        let _ = (now, timeout);
        todo!("fragment: state")
    }

    /// Whether fishnet is stalled at `now`.
    pub fn is_stalled(&self, now: SystemTime, timeout: Duration) -> bool {
        let _ = (now, timeout);
        todo!("fragment: state")
    }

    pub fn health(&self, now: SystemTime, timeout: Duration) -> Health {
        let _ = (now, timeout);
        todo!("fragment: state")
    }

    /// The JSON body of `/api/status`.
    pub fn status(&self, now: SystemTime, ctx: &StatusContext) -> Status {
        let _ = (now, ctx);
        todo!("fragment: state")
    }
}

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
