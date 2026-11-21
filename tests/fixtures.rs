//! Runs the parser over the captured logs in `tests/fixtures/`. When a new
//! fishnet release changes its output, add a capture here and these tests
//! show which lines no longer parse.

use std::{collections::BTreeMap, fs, time::Duration};

use fishnet_monitor::parser::{BatchTarget, Eval, Event, Level, LogLine, Variant, parse_line};

fn parse_fixture(name: &str) -> Vec<LogLine> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"));
    text.lines().map(parse_line).collect()
}

/// Number of lines per event kind (the variant name), plus `unparsed`.
fn tally(lines: &[LogLine]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in lines {
        let debug = format!("{:?}", line.event);
        let kind = debug
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap()
            .to_owned();
        *counts.entry(kind).or_default() += 1;
        if line.is_unparsed() {
            *counts.entry("unparsed".to_owned()).or_default() += 1;
        }
    }
    counts
}

fn expect(lines: &[LogLine], expected: &[(&str, usize)]) {
    let expected: BTreeMap<String, usize> =
        expected.iter().map(|&(k, n)| (k.to_owned(), n)).collect();
    assert_eq!(tally(lines), expected);
}

fn unparsed(lines: &[LogLine]) -> Vec<&str> {
    lines
        .iter()
        .filter(|l| l.is_unparsed())
        .map(|l| l.message.as_str())
        .collect()
}

#[test]
fn normal_run() {
    let lines = parse_fixture("normal.log");
    assert_eq!(unparsed(&lines), Vec::<&str>::new());
    expect(
        &lines,
        &[
            ("BatchFinished", 8),
            ("Backlog", 1),
            ("Cores", 1),
            ("CpuFeatures", 1),
            ("Endpoint", 1),
            ("Engines", 1),
            ("Idle", 2),
            ("Ignored", 16),
            ("Running", 1),
            ("StatsFile", 1),
            ("StoppingSoon", 1),
            ("Summary", 2),
            ("Version", 1),
        ],
    );

    let summaries: Vec<_> = lines
        .iter()
        .filter_map(|l| match &l.event {
            Event::Summary(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(summaries[1].version, "2.15.1");
    assert_eq!(summaries[1].batches, 12_411);
    assert_eq!(summaries[1].positions, 703_734);
    assert_eq!(summaries[1].nodes, 1_020_415_003_590);

    assert!(lines.iter().any(|l| l.event
        == Event::Idle {
            wait: Duration::from_secs(60)
        }));
}

#[test]
fn verbose_run() {
    let lines = parse_fixture("verbose.log");
    assert_eq!(unparsed(&lines), Vec::<&str>::new());
    expect(
        &lines,
        &[
            ("BatchFinished", 4),
            ("Backlog", 1),
            ("Cores", 1),
            ("CpuFeatures", 1),
            ("Endpoint", 1),
            ("Engines", 1),
            ("Idle", 2),
            ("Ignored", 31),
            ("Progress", 5),
            ("Running", 1),
            ("StatsFile", 1),
            ("StoppingNow", 1),
            ("Summary", 1),
            ("Version", 1),
        ],
    );

    let queued: Vec<u32> = lines
        .iter()
        .filter_map(|l| match &l.event {
            Event::Progress(p) => Some(p.queued),
            _ => None,
        })
        .collect();
    assert_eq!(queued, [9, 40, 60, 0, 0]);

    // The move batch is debug-only; analysis batches are info.
    let move_batches: Vec<_> = lines
        .iter()
        .filter(|l| l.level == Level::Debug && matches!(l.event, Event::BatchFinished(_)))
        .collect();
    assert_eq!(move_batches.len(), 1);

    let Some(Event::Summary(summary)) = lines
        .iter()
        .map(|l| &l.event)
        .find(|e| matches!(e, Event::Summary(_)))
    else {
        panic!("no summary")
    };
    assert_eq!(summary.uncertainty, 3);
}

#[test]
fn variants_hce_and_unknown_speed() {
    let lines = parse_fixture("variants.log");
    assert_eq!(unparsed(&lines), Vec::<&str>::new());
    let batches: Vec<_> = lines
        .iter()
        .map(|l| match &l.event {
            Event::BatchFinished(b) => b,
            other => panic!("{}: {other:?}", l.message),
        })
        .collect();
    let summary: Vec<(&str, Eval, Option<u32>, bool)> = batches
        .iter()
        .map(|b| {
            (
                b.variant.as_str(),
                b.eval,
                b.knps_per_core,
                b.target.game().is_some(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("standard", Eval::Nnue, Some(1342), true),
            ("antichess", Eval::Nnue, Some(910), true),
            ("atomic", Eval::Nnue, Some(875), true),
            ("crazyhouse", Eval::Nnue, Some(702), true),
            ("horde", Eval::Nnue, Some(655), true),
            ("kingOfTheHill", Eval::Nnue, Some(932), true),
            ("racingKings", Eval::Nnue, Some(1021), true),
            ("threeCheck", Eval::Nnue, Some(889), true),
            ("standard", Eval::Hce, Some(2481), true),
            ("atomic", Eval::Hce, Some(1990), true),
            ("standard", Eval::Nnue, None, true),
            ("crazyhouse", Eval::Nnue, None, true),
            ("standard", Eval::Hce, None, true),
            ("standard", Eval::Nnue, Some(1290), false),
            ("kingOfTheHill", Eval::Hce, None, false),
            ("standard", Eval::Nnue, Some(88), true),
        ]
    );
    let dev = batches.last().unwrap().target.game().unwrap();
    assert_eq!(dev.origin, "http://localhost:9663");
}

#[test]
fn problems() {
    let lines = parse_fixture("problems.log");
    assert_eq!(unparsed(&lines), Vec::<&str>::new());
    expect(
        &lines,
        &[
            ("BatchAborted", 1),
            ("EngineError", 2),
            ("EngineTimeout", 2),
            ("Fatal", 1),
            ("Ignored", 3),
            ("Other", 12),
            ("Rejected", 1),
            ("Running", 1),
            ("StoppingNow", 1),
        ],
    );
    let levels = |level| lines.iter().filter(|l| l.level == level).count();
    assert_eq!(levels(Level::Warning), 11);
    assert_eq!(levels(Level::Error), 8);
    // The fatal line comes right after the rejection.
    let fatal = lines.iter().position(|l| l.event == Event::Fatal).unwrap();
    assert!(matches!(lines[fatal - 1].event, Event::Rejected { .. }));
}

#[test]
fn panic() {
    let lines = parse_fixture("panic.log");
    assert_eq!(lines[1].event, Event::Panic);
    // The panic payload is free text and can't be recognised on its own.
    assert_eq!(
        unparsed(&lines),
        ["read key file: Os { code: 2, kind: NotFound, message: \"No such file or directory\" }"]
    );
}

#[test]
fn unknown_lines_are_tolerated() {
    let lines = parse_fixture("unknown.log");
    assert_eq!(
        unparsed(&lines),
        [
            "Something fishnet 3 might print",
            "v3.0.0: 1.4 Mnps/core (nnue), 12 batches",
            "[    |            ] https://lichess.org/Xk3pQ9aB done (1342 knps/core)",
            "Going idle for ever.",
        ]
    );
    // Known lines around them still parse.
    assert_eq!(lines[1].event, Event::Cores(4));
    let Event::BatchFinished(b) = &lines[6].event else {
        panic!("{:?}", lines[6])
    };
    assert_eq!(b.variant, Variant::Unknown("chess960".into()));
    assert!(matches!(&b.target, BatchTarget::Game(g) if g.game_id == "Xk3pQ9aB"));
    // Unknown warnings and debug lines are not format drift.
    assert!(!lines[8].is_unparsed());
    assert!(!lines[9].is_unparsed());
}
