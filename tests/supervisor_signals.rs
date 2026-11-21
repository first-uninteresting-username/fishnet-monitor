//! Signal forwarding, in its own test binary. The test sends SIGINT to this
//! process, so it must be the only test here: any other test running at the
//! same time would be hit too.

use std::time::Duration;

use fishnet_monitor::supervisor::{ChildEvent, ChildSpec, Stream, run};
use tokio::sync::mpsc;

#[tokio::test]
async fn sigint_is_forwarded_to_the_child() {
    let script = r#"trap "echo got-int; exit 7" INT; while true; do sleep 0.1; done"#;
    let spec = ChildSpec {
        command: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        forward_debug: false,
    };
    let (tx, mut rx) = mpsc::channel(1024);
    let supervisor = tokio::spawn(run(spec, tx));

    let mut events = Vec::new();
    loop {
        let event = rx.recv().await.expect("Started before the channel closes");
        let started = matches!(event, ChildEvent::Started { .. });
        events.push(event);
        if started {
            break;
        }
    }

    // `run` installs the handlers before spawning, so this is already
    // caught by the supervisor. The pause lets the shell set its trap.
    tokio::time::sleep(Duration::from_millis(200)).await;
    // SAFETY: kill(2) on our own pid with a plain signal number. This test
    // is the only one in this binary, so nothing else is signalled.
    unsafe {
        libc::kill(std::process::id() as i32, libc::SIGINT);
    }

    let code = supervisor.await.expect("supervisor task").expect("run");
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    assert_eq!(code, 7);
    assert!(events.contains(&ChildEvent::StopRequested));
    assert!(events.contains(&ChildEvent::Line {
        stream: Stream::Stdout,
        line: "got-int".to_owned(),
    }));
    assert_eq!(
        events.last(),
        Some(&ChildEvent::Exited {
            code: Some(7),
            signal: None
        })
    );
}
