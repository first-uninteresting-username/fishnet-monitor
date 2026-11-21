//! `/metrics` in the Prometheus text exposition format (version 0.0.4),
//! rendered by hand from [`State`].

use std::{
    fmt::{Display, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::state::{KNPS_BUCKETS, RunState, State};

/// Content type of [`render`]'s output.
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Renders all metrics from the table in `PLAN.md` that phase 2 has data
/// for (everything except the game-lookup metrics).
pub fn render(state: &State, now: SystemTime, stall_timeout: Duration) -> String {
    render_with(state, state.is_stalled(now, stall_timeout))
}

/// The body of [`render`], with the stall check already done by the caller.
/// Tests use this so they don't depend on `State::is_stalled`.
fn render_with(state: &State, stalled: bool) -> String {
    let mut out = String::new();

    gauge(
        &mut out,
        "fishnet_up",
        "1 while the fishnet child process is alive.",
    );
    sample(&mut out, "fishnet_up", &[], u8::from(state.child_running));

    if let Some(version) = &state.version {
        gauge(
            &mut out,
            "fishnet_info",
            "fishnet version from the summary line.",
        );
        sample(&mut out, "fishnet_info", &[("version", version)], 1);
    }

    counter(
        &mut out,
        "fishnet_batches_total",
        "Batches completed, all time.",
    );
    sample(&mut out, "fishnet_batches_total", &[], state.totals.batches);
    counter(
        &mut out,
        "fishnet_positions_total",
        "Positions analysed, all time.",
    );
    sample(
        &mut out,
        "fishnet_positions_total",
        &[],
        state.totals.positions,
    );
    counter(&mut out, "fishnet_nodes_total", "Nodes searched, all time.");
    sample(&mut out, "fishnet_nodes_total", &[], state.totals.nodes);

    counter(
        &mut out,
        "fishnet_batches_finished_total",
        "Finished batches by variant and eval type.",
    );
    for ((variant, eval), n) in &state.batches_finished {
        sample(
            &mut out,
            "fishnet_batches_finished_total",
            &[("variant", variant.as_str()), ("eval", eval.as_str())],
            n,
        );
    }

    if let Some(knps) = state.last_knps_per_core {
        gauge(
            &mut out,
            "fishnet_knps_per_core",
            "Speed of the last finished batch, in knps per core.",
        );
        sample(&mut out, "fishnet_knps_per_core", &[], knps);
    }
    histogram(
        &mut out,
        "fishnet_knps_per_core_histogram",
        "Speed of finished batches, in knps per core.",
    );
    for (bound, count) in KNPS_BUCKETS.iter().zip(&state.knps_histogram.counts) {
        let le = bound.to_string();
        sample(
            &mut out,
            "fishnet_knps_per_core_histogram_bucket",
            &[("le", le.as_str())],
            count,
        );
    }
    let hist = &state.knps_histogram;
    sample(
        &mut out,
        "fishnet_knps_per_core_histogram_bucket",
        &[("le", "+Inf")],
        hist.count,
    );
    sample(
        &mut out,
        "fishnet_knps_per_core_histogram_sum",
        &[],
        hist.sum,
    );
    sample(
        &mut out,
        "fishnet_knps_per_core_histogram_count",
        &[],
        hist.count,
    );

    if let Some(nnue) = state.nnue_knps_per_core {
        gauge(
            &mut out,
            "fishnet_nnue_knps_per_core",
            "NNUE speed from the summary line, in knps per core.",
        );
        sample(&mut out, "fishnet_nnue_knps_per_core", &[], nnue);
    }

    if let Some(at) = state.last_batch_at {
        gauge(
            &mut out,
            "fishnet_last_batch_timestamp_seconds",
            "Unix time of the last finished batch.",
        );
        sample(
            &mut out,
            "fishnet_last_batch_timestamp_seconds",
            &[],
            unix_seconds(at),
        );
    }

    gauge(
        &mut out,
        "fishnet_state",
        "Run state: 1 for the current state, 0 for the others.",
    );
    for run_state in [
        RunState::Starting,
        RunState::Running,
        RunState::Idle,
        RunState::Stopping,
        RunState::Stopped,
    ] {
        sample(
            &mut out,
            "fishnet_state",
            &[("state", run_state.as_str())],
            u8::from(run_state == state.run_state),
        );
    }

    if let Some(pending) = state.queue_pending {
        gauge(
            &mut out,
            "fishnet_queue_pending",
            "Batches waiting in the queue (from the progress line, needs -v).",
        );
        sample(&mut out, "fishnet_queue_pending", &[], pending);
    }
    if let Some(cores) = state.cores {
        gauge(&mut out, "fishnet_cores", "Number of cores fishnet uses.");
        sample(&mut out, "fishnet_cores", &[], cores);
    }

    counter(
        &mut out,
        "fishnet_log_lines_total",
        "Log lines from fishnet, by level.",
    );
    for (level, n) in &state.log_lines {
        sample(&mut out, "fishnet_log_lines_total", &[("level", level)], n);
    }

    counter(
        &mut out,
        "fishnet_engine_errors_total",
        "Engine errors reported by fishnet (E: lines).",
    );
    sample(
        &mut out,
        "fishnet_engine_errors_total",
        &[],
        state.engine_errors,
    );
    counter(
        &mut out,
        "fishnet_engine_timeouts_total",
        "Engine timeouts reported by fishnet.",
    );
    sample(
        &mut out,
        "fishnet_engine_timeouts_total",
        &[],
        state.engine_timeouts,
    );
    counter(
        &mut out,
        "fishnet_batches_aborted_total",
        "Batches aborted (W: lines).",
    );
    sample(
        &mut out,
        "fishnet_batches_aborted_total",
        &[],
        state.batches_aborted,
    );
    counter(
        &mut out,
        "fishnet_unparsed_lines_total",
        "Log lines that matched no parser, to spot format drift.",
    );
    sample(
        &mut out,
        "fishnet_unparsed_lines_total",
        &[],
        state.unparsed_lines,
    );

    gauge(
        &mut out,
        "fishnet_fatal",
        "1 once fishnet has stopped with a fatal error.",
    );
    sample(&mut out, "fishnet_fatal", &[], u8::from(state.fatal));

    gauge(
        &mut out,
        "fishnet_stalled",
        "1 when there has been no batch or idle line for longer than the stall timeout.",
    );
    sample(&mut out, "fishnet_stalled", &[], u8::from(stalled));

    out
}

fn gauge(out: &mut String, name: &str, help: &str) {
    metadata(out, name, "gauge", help);
}

fn counter(out: &mut String, name: &str, help: &str) {
    metadata(out, name, "counter", help);
}

fn histogram(out: &mut String, name: &str, help: &str) {
    metadata(out, name, "histogram", help);
}

/// Writes the `# HELP` and `# TYPE` lines for one metric family.
fn metadata(out: &mut String, name: &str, kind: &str, help: &str) {
    // Writing to a String cannot fail, so the fmt results are ignored.
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

/// Writes one sample line: `name{label="value",...} value`.
fn sample(out: &mut String, name: &str, labels: &[(&str, &str)], value: impl Display) {
    out.push_str(name);
    if !labels.is_empty() {
        out.push('{');
        for (i, (key, val)) in labels.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(key);
            out.push_str("=\"");
            escape_label_value(out, val);
            out.push('"');
        }
        out.push('}');
    }
    let _ = writeln!(out, " {value}");
}

/// Escapes a label value as the exposition format requires: backslash,
/// double quote and newline.
fn escape_label_value(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            other => out.push(other),
        }
    }
}

/// Seconds since the Unix epoch; 0 for times before it.
fn unix_seconds(t: SystemTime) -> f64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::state::Totals;

    fn epoch_plus(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    /// A state with every optional metric set and a few values in each family.
    fn populated() -> State {
        let mut state = State::new(epoch_plus(1_000));
        state.child_running = true;
        state.run_state = RunState::Idle;
        state.version = Some("2.10.0".to_owned());
        state.cores = Some(4);
        state.queue_pending = Some(7);
        state.nnue_knps_per_core = Some(1234);
        state.last_knps_per_core = Some(987);
        state.last_batch_at = Some(UNIX_EPOCH + Duration::from_millis(1_700_000_000_500));
        state.totals = Totals {
            batches: 10,
            positions: 2000,
            nodes: 3_000_000,
        };
        state
            .batches_finished
            .insert(("standard".to_owned(), "nnue".to_owned()), 3);
        state
            .batches_finished
            .insert(("chess960".to_owned(), "hce".to_owned()), 1);
        state.knps_histogram.observe(50.0);
        state.knps_histogram.observe(800.0);
        state.knps_histogram.observe(6000.0);
        state.log_lines.insert("info", 5);
        state.log_lines.insert("warn", 2);
        state.engine_errors = 1;
        state.engine_timeouts = 2;
        state.batches_aborted = 3;
        state.unparsed_lines = 4;
        state
    }

    /// True if some sample line starts with `prefix` (metadata lines excluded).
    fn has_sample(out: &str, prefix: &str) -> bool {
        out.lines()
            .any(|line| !line.starts_with('#') && line.starts_with(prefix))
    }

    #[test]
    fn populated_state_renders_exact_sample_lines() {
        let out = render_with(&populated(), true);
        let expected = [
            "fishnet_up 1\n",
            "fishnet_info{version=\"2.10.0\"} 1\n",
            "fishnet_batches_total 10\n",
            "fishnet_positions_total 2000\n",
            "fishnet_nodes_total 3000000\n",
            "fishnet_batches_finished_total{variant=\"standard\",eval=\"nnue\"} 3\n",
            "fishnet_batches_finished_total{variant=\"chess960\",eval=\"hce\"} 1\n",
            "fishnet_knps_per_core 987\n",
            "fishnet_nnue_knps_per_core 1234\n",
            "fishnet_last_batch_timestamp_seconds 1700000000.5\n",
            "fishnet_state{state=\"idle\"} 1\n",
            "fishnet_state{state=\"running\"} 0\n",
            "fishnet_state{state=\"stopped\"} 0\n",
            "fishnet_queue_pending 7\n",
            "fishnet_cores 4\n",
            "fishnet_log_lines_total{level=\"warn\"} 2\n",
            "fishnet_engine_errors_total 1\n",
            "fishnet_engine_timeouts_total 2\n",
            "fishnet_batches_aborted_total 3\n",
            "fishnet_unparsed_lines_total 4\n",
            "fishnet_fatal 0\n",
            "fishnet_stalled 1\n",
        ];
        for line in expected {
            assert!(out.contains(line), "missing {line:?} in:\n{out}");
        }
    }

    #[test]
    fn histogram_buckets_are_cumulative_and_end_with_inf() {
        let out = render_with(&populated(), false);
        // 50 falls in le=100; 800 in le=1000; 6000 only in +Inf.
        assert!(out.contains("fishnet_knps_per_core_histogram_bucket{le=\"100\"} 1\n"));
        assert!(out.contains("fishnet_knps_per_core_histogram_bucket{le=\"1000\"} 2\n"));
        assert!(out.contains("fishnet_knps_per_core_histogram_bucket{le=\"5000\"} 2\n"));
        assert!(out.contains("fishnet_knps_per_core_histogram_bucket{le=\"+Inf\"} 3\n"));
        assert!(out.contains("fishnet_knps_per_core_histogram_sum 6850\n"));
        assert!(out.contains("fishnet_knps_per_core_histogram_count 3\n"));

        let counts: Vec<u64> = out
            .lines()
            .filter(|l| l.starts_with("fishnet_knps_per_core_histogram_bucket{"))
            .map(|l| l.rsplit(' ').next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(counts.len(), KNPS_BUCKETS.len() + 1);
        assert!(counts.windows(2).all(|w| w[0] <= w[1]), "{counts:?}");
        assert_eq!(*counts.last().unwrap(), 3);
    }

    #[test]
    fn optional_metrics_are_omitted_when_none() {
        let out = render_with(&State::new(epoch_plus(1_000)), false);
        assert!(!out.contains("fishnet_info"));
        assert!(!has_sample(&out, "fishnet_knps_per_core "));
        assert!(!has_sample(&out, "fishnet_nnue_knps_per_core"));
        assert!(!has_sample(&out, "fishnet_last_batch_timestamp_seconds"));
        assert!(!has_sample(&out, "fishnet_queue_pending"));
        assert!(!has_sample(&out, "fishnet_cores"));
        // The histogram is always there, even with no observations.
        assert!(out.contains("fishnet_knps_per_core_histogram_bucket{le=\"+Inf\"} 0\n"));
        assert!(out.contains("fishnet_up 0\n"));
        assert!(out.contains("fishnet_state{state=\"starting\"} 1\n"));
    }

    #[test]
    fn version_label_is_escaped() {
        let mut state = State::new(epoch_plus(1_000));
        state.version = Some("a\"b\\c\nd".to_owned());
        let out = render_with(&state, false);
        // Raw newline would split the sample, so it must come out as `\n`.
        assert!(
            out.contains(r#"fishnet_info{version="a\"b\\c\nd"} 1"#),
            "{out}"
        );
    }

    #[test]
    fn every_sample_has_a_preceding_type_line() {
        let out = render_with(&populated(), false);
        let mut declared: HashSet<&str> = HashSet::new();
        for line in out.lines() {
            if let Some(rest) = line.strip_prefix("# TYPE ") {
                declared.insert(rest.split(' ').next().unwrap());
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            let name_end = line.find(['{', ' ']).expect("sample has a value");
            let name = &line[..name_end];
            let base = ["_bucket", "_sum", "_count"]
                .iter()
                .find_map(|suffix| name.strip_suffix(suffix))
                .filter(|base| declared.contains(base));
            assert!(
                declared.contains(name) || base.is_some(),
                "no earlier # TYPE for {name}"
            );
        }
    }

    #[test]
    fn stalled_flag_is_passed_through() {
        assert!(render_with(&State::new(epoch_plus(1)), true).contains("fishnet_stalled 1\n"));
        assert!(render_with(&State::new(epoch_plus(1)), false).contains("fishnet_stalled 0\n"));
    }

    #[test]
    fn output_ends_with_newline() {
        let out = render_with(&populated(), false);
        assert!(out.ends_with('\n'));
        assert!(!out.contains("\n\n"));
    }
}
