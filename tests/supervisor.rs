//! Runs the supervisor against small `sh -c` children and checks the events
//! and the returned exit code. Signal forwarding is tested in
//! `tests/supervisor_signals.rs`, which runs in its own process.

use fishnet_monitor::supervisor::{ChildEvent, ChildSpec, Stream, run};
use tokio::sync::mpsc;

fn sh(script: &str) -> ChildSpec {
    ChildSpec {
        command: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        forward_debug: false,
    }
}

/// Runs `spec` to completion and returns the result with every event, in
/// order. The channel is larger than any test's event count, so `run` never
/// blocks on a full channel.
async fn run_collect(spec: ChildSpec) -> (std::io::Result<i32>, Vec<ChildEvent>) {
    let (tx, mut rx) = mpsc::channel(1024);
    let result = run(spec, tx).await;
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    (result, events)
}

fn lines(events: &[ChildEvent], stream: Stream) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            ChildEvent::Line { stream: s, line } if *s == stream => Some(line.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn stdout_and_stderr_lines_arrive_in_order_per_stream() {
    let script = r#"
        printf 'a\nb\n'
        printf 'e1\n' >&2
        printf 'c\n'
        printf 'e2\r\n' >&2
        printf '\377abc\n'
        printf 'D: debug\n'
        printf 'tail'
    "#;
    let (result, events) = run_collect(sh(script)).await;

    assert_eq!(result.unwrap(), 0);
    assert_eq!(
        lines(&events, Stream::Stdout),
        ["a", "b", "c", "\u{FFFD}abc", "D: debug", "tail"]
    );
    // The trailing `\r` is stripped.
    assert_eq!(lines(&events, Stream::Stderr), ["e1", "e2"]);
}

#[tokio::test]
async fn exit_code_is_passed_on() {
    let (result, events) = run_collect(sh("exit 3")).await;

    assert_eq!(result.unwrap(), 3);
    assert_eq!(
        events.last(),
        Some(&ChildEvent::Exited {
            code: Some(3),
            signal: None
        })
    );
}

#[tokio::test]
async fn death_by_signal_is_reported_as_128_plus_signal() {
    let (result, events) = run_collect(sh("kill -TERM $$")).await;

    assert_eq!(result.unwrap(), 143);
    assert_eq!(
        events.last(),
        Some(&ChildEvent::Exited {
            code: None,
            signal: Some(15)
        })
    );
}

#[tokio::test]
async fn started_comes_first_and_exited_last() {
    let (result, events) = run_collect(sh("echo hi")).await;

    assert_eq!(result.unwrap(), 0);
    assert!(matches!(events.first(), Some(ChildEvent::Started { .. })));
    assert!(matches!(events.last(), Some(ChildEvent::Exited { .. })));
    let exits = events
        .iter()
        .filter(|e| matches!(e, ChildEvent::Exited { .. }))
        .count();
    assert_eq!(exits, 1);
}

#[tokio::test]
async fn empty_command_is_an_error() {
    let spec = ChildSpec {
        command: Vec::new(),
        forward_debug: false,
    };
    let (result, events) = run_collect(spec).await;

    let err = result.expect_err("empty command must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(events.is_empty());
}
