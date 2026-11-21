//! `docker stop` sends SIGTERM to the monitor (PID 1), which forwards it to
//! fishnet. fishnet stops at once, and the monitor must still save the
//! history snapshot before it exits. This runs the real binary, in its own
//! test file because it signals a child process by pid.

use std::{
    io::{BufRead as _, BufReader},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
fn sigterm_still_saves_the_history_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("history.json");

    // A stand-in for fishnet: one batch, then wait to be stopped. `exec`
    // makes `sleep` the signalled process, so the child dies of SIGTERM
    // like fishnet does.
    let script = "echo '[    |            ] https://lichess.org/Xk3pQ9aB finished (1342 knps/core)'; \
                  echo ready; exec sleep 60";
    let mut monitor = Command::new(env!("CARGO_BIN_EXE_fishnet-monitor"))
        .args(["run", "--", "sh", "-c", script])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HISTORY_FILE", &history)
        .env("MONITOR_LISTEN", "127.0.0.1:0")
        .env("GAME_LOOKUP", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // The forwarded "ready" means the child runs, so the monitor's signal
    // handlers are installed (they go in before the spawn).
    let stdout = monitor.stdout.take().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.as_deref().is_ok_and(|l| l == "ready") {
                let _ = ready_tx.send(());
            }
        }
    });
    ready_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("child output is forwarded");
    // The 60 s snapshot timer hasn't fired yet, so a file now can only come
    // from the shutdown save.
    assert!(!history.exists());

    // SAFETY: kill(2) on the pid of a child we spawned, with a plain signal.
    unsafe {
        libc::kill(monitor.id() as i32, libc::SIGTERM);
    }
    let status = wait_timeout(&mut monitor, Duration::from_secs(10));

    // Killed by SIGTERM is passed on as 128 + 15.
    assert_eq!(status.code(), Some(143));
    let saved = std::fs::read_to_string(&history).expect("history saved on shutdown");
    let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(json["version"], 1);
    assert!(saved.contains("Xk3pQ9aB"), "the batch is in the snapshot");
}

fn wait_timeout(child: &mut std::process::Child, limit: Duration) -> std::process::ExitStatus {
    let step = Duration::from_millis(50);
    let mut waited = Duration::ZERO;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if waited >= limit {
            let _ = child.kill();
            panic!("monitor didn't exit within {limit:?} after SIGTERM");
        }
        thread::sleep(step);
        waited += step;
    }
}
