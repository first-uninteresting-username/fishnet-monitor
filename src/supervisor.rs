//! Runs fishnet as a child process: forwards its output, forwards signals,
//! reaps zombies (the monitor is PID 1 in the container) and reports the
//! exit status.

use tokio::sync::mpsc;

/// Which stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// What the supervisor reports while the child runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildEvent {
    Started {
        pid: u32,
    },
    /// One output line, without the trailing newline (invalid UTF-8 is
    /// replaced lossily).
    Line {
        stream: Stream,
        line: String,
    },
    /// SIGINT or SIGTERM was received and forwarded to the child.
    StopRequested,
    /// The child exited. `code` is `None` if it was killed by a signal;
    /// `signal` is that signal number.
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

pub struct ChildSpec {
    /// Program and arguments, for example `["/docker-entrypoint.sh"]`.
    pub command: Vec<String>,
    /// Forward lines starting with `D: ` to our stdout/stderr. All lines
    /// are always sent as [`ChildEvent::Line`].
    pub forward_debug: bool,
}

/// Spawns the child and supervises it until it exits.
///
/// - stdin is inherited, stdout and stderr are piped. Each line is written
///   unchanged to our own stdout/stderr respectively (except `D: ` lines
///   unless `forward_debug`), flushed per line, and sent as a `Line` event.
///   Forwarding must not wait for the channel consumer to be slow: use
///   `send().await` on a bounded channel only after writing the line out.
/// - SIGINT and SIGTERM received by the monitor are forwarded to the child
///   (every time, so a second SIGINT aborts as fishnet expects), each
///   followed by a `StopRequested` event.
/// - If running as PID 1, also reaps orphaned zombies (`waitpid(-1,
///   WNOHANG)` on SIGCHLD) without stealing the child's own exit status.
/// - After the child exits, drains the remaining output, then sends
///   `Exited` and returns the exit code to pass on: the child's code, or
///   `128 + signal` if it was killed by a signal.
///
/// Returns an error if the command is empty or can't be spawned.
pub async fn run(spec: ChildSpec, events: mpsc::Sender<ChildEvent>) -> std::io::Result<i32> {
    let _ = (spec, events);
    todo!("fragment: supervisor")
}
