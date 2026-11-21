use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, Write as _},
    process::ExitCode,
};

use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use fishnet_monitor::{
    config::Config,
    history::ProblemLevel,
    http::{self, AppState},
    lookup::{self, Queue, Source},
    parser::{Event, Level, parse_line},
    snapshot,
    state::{Alert, State, StatusContext},
    stats_file,
    supervisor::{self, ChildEvent, ChildSpec},
    view::{Changes, Topic},
    webhook::{self, Delivery},
};
use tokio::sync::mpsc;

const USAGE: &str = "\
usage: fishnet-monitor <command>

commands:
  run [--] CMD...  run CMD (the fishnet entrypoint) and monitor it
  healthcheck      exit 0 if the monitor on MONITOR_LISTEN reports healthy
  parse [FILE]   parse fishnet log lines from FILE (or stdin) and print the
                 events, to check the parser against real output";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => {
            let rest = &args[1..];
            let command = match rest.first().map(String::as_str) {
                Some("--") => &rest[1..],
                _ => rest,
            };
            run(command.to_vec())
        }
        Some("healthcheck") => healthcheck(),
        Some("parse") => parse(args.get(1).map(String::as_str)),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// How often the stall check and the history's time accounting run.
const STALL_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// How often the stats file is polled.
const STATS_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// How long to wait for webhooks still being delivered after fishnet exits.
const WEBHOOK_DRAIN_TIMEOUT: Duration = Duration::from_secs(40);

fn config() -> Result<Config, ExitCode> {
    Config::from_env().map_err(|err| {
        eprintln!("fishnet-monitor: {err}");
        ExitCode::from(2)
    })
}

fn healthcheck() -> ExitCode {
    let config = match config() {
        Ok(config) => config,
        Err(code) => return code,
    };
    if http::healthcheck(config.listen) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(command: Vec<String>) -> ExitCode {
    if command.is_empty() {
        eprintln!("fishnet-monitor: run: no command given\n{USAGE}");
        return ExitCode::from(2);
    }
    let config = match config() {
        Ok(config) => config,
        Err(code) => return code,
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("fishnet-monitor: can't start the runtime: {err}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(supervise(config, command));
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

async fn supervise(config: Config, command: Vec<String>) -> i32 {
    let state = Arc::new(Mutex::new(State::new(SystemTime::now())));
    let changes = Arc::new(Changes::default());
    state.lock().unwrap().lookup = Queue::new(config.game_lookup, &config.lichess_api);
    if config.game_lookup {
        tokio::spawn(lookup::run(
            Arc::clone(&state),
            Arc::clone(&changes),
            Source::http(&config.lichess_api),
        ));
    }
    if let Some(path) = &config.history_file {
        let now = SystemTime::now();
        match snapshot::load(path, now) {
            Ok(Some(history)) => state.lock().unwrap().history = history,
            Ok(None) => {}
            Err(err) => {
                let msg = format!("fishnet-monitor: {err}");
                eprintln!("W: {msg}");
                state
                    .lock()
                    .unwrap()
                    .history
                    .record_problem(now, ProblemLevel::W, &msg);
            }
        }
        tokio::spawn(snapshot::run(path.clone(), Arc::clone(&state)));
    }
    let (alert_tx, alert_rx) = mpsc::channel::<Alert>(16);

    let webhook = config.webhook_url.clone().map(|url| {
        // A final failure goes to our stderr in fishnet's warning format and
        // into the dashboard's problems list.
        let state = Arc::clone(&state);
        let changes = Arc::clone(&changes);
        tokio::spawn(webhook::run(
            url,
            config.host_name.clone(),
            Delivery::default(),
            alert_rx,
            move |msg| {
                let msg = format!("fishnet-monitor: {msg}");
                eprintln!("W: {msg}");
                state.lock().unwrap().history.record_problem(
                    SystemTime::now(),
                    ProblemLevel::W,
                    &msg,
                );
                changes.bump(Topic::Live);
            },
        ))
    });

    if let Some(path) = config.stats_file.clone() {
        let changes = Arc::clone(&changes);
        tokio::spawn(stats_file::watch(
            path,
            STATS_POLL_INTERVAL,
            Arc::clone(&state),
            move || changes.bump(Topic::Live),
        ));
    }

    let app = AppState {
        state: Arc::clone(&state),
        ctx: Arc::new(StatusContext {
            host: config.host_name.clone(),
            listen: config.listen.to_string(),
            stall_timeout: config.stall_timeout,
        }),
        changes: Arc::clone(&changes),
    };
    let listen = config.listen;
    tokio::spawn(async move {
        if let Err(err) = http::serve(listen, app).await {
            eprintln!("W: fishnet-monitor: can't serve on {listen}: {err}");
        }
    });

    let stall = {
        let state = Arc::clone(&state);
        let alerts = alert_tx.clone();
        let changes = Arc::clone(&changes);
        let timeout = config.stall_timeout;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(STALL_CHECK_INTERVAL);
            loop {
                tick.tick().await;
                let now = SystemTime::now();
                let (alert, new_minute) = {
                    let mut state = state.lock().unwrap();
                    let new_minute = state.tick(now);
                    (state.check_stall(now, timeout), new_minute)
                };
                if new_minute {
                    // New chart point; "today" and the 24 h figures move on.
                    changes.bump_all(&[Topic::Charts, Topic::Live, Topic::Lookup]);
                }
                if alert.is_some() {
                    changes.bump(Topic::Status);
                }
                if let Some(alert) = alert {
                    let _ = alerts.try_send(alert);
                }
            }
        })
    };

    let (event_tx, mut event_rx) = mpsc::channel::<ChildEvent>(1024);
    let spec = ChildSpec {
        command,
        forward_debug: config.forward_debug,
    };
    let child = tokio::spawn(supervisor::run(spec, event_tx));

    let mut requested = false;
    while let Some(event) = event_rx.recv().await {
        let now = SystemTime::now();
        let alert = {
            let mut state = state.lock().unwrap();
            match event {
                ChildEvent::Started { .. } => {
                    state.child_started(now);
                    None
                }
                ChildEvent::Line { line, .. } => {
                    let line = parse_line(&line);
                    if matches!(line.event, Event::BatchFinished(_)) {
                        changes.bump_all(&[Topic::Live, Topic::Lookup]);
                    } else if matches!(line.level, Level::Warning | Level::Error) {
                        changes.bump(Topic::Live);
                    }
                    state.apply_line(&line, now)
                }
                ChildEvent::StopRequested => {
                    requested = true;
                    state.stop_requested();
                    None
                }
                ChildEvent::Exited { code, .. } => state.child_exited(code, requested, now),
            }
        };
        changes.bump(Topic::Status);
        if let Some(alert) = alert {
            changes.bump(Topic::Live);
            let _ = alert_tx.try_send(alert);
        }
    }

    let code = match child.await {
        Ok(Ok(code)) => code,
        Ok(Err(err)) => {
            eprintln!("fishnet-monitor: can't run the child: {err}");
            127
        }
        Err(err) => {
            eprintln!("fishnet-monitor: supervisor failed: {err}");
            1
        }
    };

    // Save the history and let a final fatal webhook go out before exiting.
    stall.abort();
    state.lock().unwrap().tick(SystemTime::now());
    if let Some(path) = &config.history_file {
        snapshot::save(path, &state).await;
    }
    drop(alert_tx);
    if let Some(webhook) = webhook
        && tokio::time::timeout(WEBHOOK_DRAIN_TIMEOUT, webhook)
            .await
            .is_err()
    {
        eprintln!("W: fishnet-monitor: gave up waiting for webhook delivery");
    }
    code
}

/// Prints one line per input line: level, unparsed marker and event. Exits
/// with 1 if any line was unparsed, so it can be used on new fishnet output.
fn parse(path: Option<&str>) -> ExitCode {
    let input: Box<dyn BufRead> = match path {
        None | Some("-") => Box::new(io::stdin().lock()),
        Some(path) => match File::open(path) {
            Ok(file) => Box::new(BufReader::new(file)),
            Err(err) => {
                eprintln!("fishnet-monitor: {path}: {err}");
                return ExitCode::from(2);
            }
        },
    };

    let mut out = io::stdout().lock();
    let (mut total, mut unparsed) = (0usize, 0usize);
    for line in input.split(b'\n') {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                eprintln!("fishnet-monitor: read error: {err}");
                return ExitCode::from(2);
            }
        };
        let parsed = parse_line(&String::from_utf8_lossy(&line));
        total += 1;
        let marker = if parsed.is_unparsed() {
            unparsed += 1;
            "?"
        } else {
            " "
        };
        if writeln!(
            out,
            "{:<7} {marker} {:?}",
            parsed.level.as_str(),
            parsed.event
        )
        .is_err()
        {
            break;
        }
    }
    eprintln!("{total} lines, {unparsed} unparsed");
    if unparsed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
