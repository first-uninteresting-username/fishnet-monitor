//! Runs fishnet as a child process: forwards its output, forwards signals,
//! reaps zombies (the monitor is PID 1 in the container) and reports the
//! exit status.

use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt as _;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

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
    let (program, args) = spec
        .command
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty child command"))?;

    // Install the handlers before spawning: a signal sent as soon as
    // `Started` arrives must reach us, not hit the default disposition.
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigchld = if std::process::id() == 1 {
        Some(signal(SignalKind::child())?)
    } else {
        None
    };

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false)
        .spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| io::Error::other("child has no pid"))?;
    // The receiver may already be gone; supervision goes on regardless.
    let _ = events.send(ChildEvent::Started { pid }).await;

    // Both pipes were requested as piped above, so they are present.
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout_reader = spawn_reader(stdout, Stream::Stdout, spec.forward_debug, events.clone());
    let stderr_reader = spawn_reader(stderr, Stream::Stderr, spec.forward_debug, events.clone());

    let status = loop {
        tokio::select! {
            status = child.wait() => break status?,
            _ = sigint.recv() => forward_signal(pid, libc::SIGINT, &events).await,
            _ = sigterm.recv() => forward_signal(pid, libc::SIGTERM, &events).await,
            () = next_signal(&mut sigchld) => reap_orphans(pid),
        }
    };

    // Drain the remaining output before reporting the exit. A reader that
    // panicked has nothing more to send, so its join error is ignored.
    let _ = tokio::join!(stdout_reader, stderr_reader);

    let code = status.code();
    let signo = status.signal();
    let _ = events
        .send(ChildEvent::Exited {
            code,
            signal: signo,
        })
        .await;
    Ok(code.unwrap_or_else(|| 128 + signo.unwrap_or(0)))
}

/// Reads `pipe` line by line in a task. Each line is written to our own
/// output (unless it is a `D: ` line and debug forwarding is off), then sent
/// as a `Line` event.
fn spawn_reader<R>(
    pipe: R,
    stream: Stream,
    forward_debug: bool,
    events: mpsc::Sender<ChildEvent>,
) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut segments = BufReader::new(pipe).split(b'\n');
        loop {
            let mut bytes = match segments.next_segment().await {
                Ok(Some(bytes)) => bytes,
                Ok(None) => break,
                Err(err) => {
                    eprintln!("supervisor: reading child {stream:?}: {err}");
                    break;
                }
            };
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            let line = String::from_utf8_lossy(&bytes).into_owned();
            if forward_debug || !line.starts_with("D: ") {
                write_line(stream, &line);
            }
            // The receiver may be gone; keep draining the pipe anyway.
            let _ = events.send(ChildEvent::Line { stream, line }).await;
        }
    })
}

/// Writes `line` plus a newline to our stdout or stderr, then flushes.
///
/// This uses blocking std writes. A short write to a terminal or log pipe
/// is fast, and tokio's stdio needs the `io-std` feature, which is not
/// enabled. Write errors are ignored on purpose: if our own output is closed
/// there is nothing useful to do, and the child must still be drained so it
/// never blocks on a full pipe.
fn write_line(stream: Stream, line: &str) {
    let _ = match stream {
        Stream::Stdout => write_and_flush(io::stdout().lock(), line),
        Stream::Stderr => write_and_flush(io::stderr().lock(), line),
    };
}

fn write_and_flush<W: Write>(mut out: W, line: &str) -> io::Result<()> {
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Forwards `signo` to the child, then reports it. A failed `kill` (for
/// example ESRCH, if the child is already gone) is ignored: there is
/// nothing left to stop.
async fn forward_signal(pid: u32, signo: i32, events: &mpsc::Sender<ChildEvent>) {
    // SAFETY: kill(2) takes plain integers and has no memory-safety
    // preconditions.
    unsafe {
        libc::kill(pid as i32, signo);
    }
    let _ = events.send(ChildEvent::StopRequested).await;
}

/// Completes on each signal from `sig`, or never if there is no stream.
/// Lets the `select!` loop have an optional SIGCHLD branch.
async fn next_signal(sig: &mut Option<Signal>) {
    match sig {
        Some(sig) => {
            sig.recv().await;
        }
        None => std::future::pending().await,
    }
}

/// Reaps exited children other than `child_pid`. Only called when we are
/// PID 1, so orphans have been re-parented to us.
///
/// A plain `waitpid(-1)` would also reap our own child, and tokio could then
/// not read its exit status. So first peek with `waitid(.., WNOWAIT)`, which
/// reports the next waitable child without consuming its status. If that
/// child is ours, stop and let tokio reap it. Otherwise reap it and peek
/// again. Note that the peek returns children one at a time, so orphans
/// queued behind our child are only reaped after it has exited.
fn reap_orphans(child_pid: u32) {
    loop {
        // SAFETY: siginfo_t is plain old data, so all-zero is a valid value.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a valid, writable siginfo_t.
        let rc = unsafe {
            libc::waitid(
                libc::P_ALL,
                0,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        // A nonzero rc means no children are left (ECHILD). si_pid 0 means
        // no child has changed state yet.
        // SAFETY: after a successful waitid, `info` holds a valid siginfo.
        let pid = if rc == 0 { unsafe { info.si_pid() } } else { 0 };
        if pid == 0 || pid == child_pid as i32 {
            break;
        }
        let mut status = 0;
        // SAFETY: valid out-pointer for the status. The child was just
        // reported as waitable, so WNOHANG reaps it without blocking.
        unsafe {
            libc::waitpid(pid, &mut status, libc::WNOHANG);
        }
    }
}
