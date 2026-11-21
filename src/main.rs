use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, Write as _},
    process::ExitCode,
};

use fishnet_monitor::parser::parse_line;

const USAGE: &str = "\
usage: fishnet-monitor <command>

commands:
  parse [FILE]   parse fishnet log lines from FILE (or stdin) and print the
                 events, to check the parser against real output";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
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
