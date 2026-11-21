//! Saves the [`History`] to `HISTORY_FILE` (`/data/history.json` in the
//! image) every minute and on shutdown, so the charts survive a restart.
//! The file is ours; fishnet never touches it.
//!
//! Format: `{"version": 1, "savedAt": <epoch ms>, "history": {...}}`. It is
//! written to a temporary file next to it and renamed, so a crash never
//! leaves half a file. A file that can't be read is moved aside to
//! `<name>.bad` and a new history starts.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{
    history::{History, ProblemLevel, millis, secs},
    state::State,
};

/// The current file format.
pub const VERSION: u32 = 1;

/// How often [`run`] saves.
pub const INTERVAL: Duration = Duration::from_secs(60);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Out<'a> {
    version: u32,
    saved_at: u64,
    history: &'a History,
}

#[derive(Deserialize)]
struct In {
    version: u32,
    history: History,
}

pub fn encode(history: &History, now: SystemTime) -> Vec<u8> {
    serde_json::to_vec(&Out {
        version: VERSION,
        saved_at: millis(now),
        history,
    })
    .expect("history serializes")
}

/// Reads the file. `Ok(None)` when it doesn't exist yet. On an unreadable
/// file, it is renamed to `<name>.bad` and the error says so.
pub fn load(path: &Path, now: SystemTime) -> Result<Option<History>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("can't read {}: {err}", path.display())),
    };
    let problem = match serde_json::from_slice::<In>(&bytes) {
        Ok(file) if file.version == VERSION => {
            let mut history = file.history;
            history.prune(secs(now));
            return Ok(Some(history));
        }
        Ok(file) => format!("unknown format version {}", file.version),
        Err(err) => err.to_string(),
    };
    let bad = bad_path(path);
    let kept = match fs::rename(path, &bad) {
        Ok(()) => format!("kept as {}", bad.display()),
        Err(err) => format!("can't move it aside: {err}"),
    };
    Err(format!(
        "history file {} is unreadable ({problem}); starting a new history, old file {kept}",
        path.display()
    ))
}

fn bad_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bad");
    path.with_file_name(name)
}

/// Writes `bytes` to a temporary file in the same directory, syncs it and
/// renames it over `path`.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    let result = (|| {
        let file = fs::File::create(&tmp)?;
        io::Write::write_all(&mut &file, bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Saves the state's history once. A failure goes into the problems list.
pub async fn save(path: &Path, state: &Arc<Mutex<State>>) {
    let now = SystemTime::now();
    // Encode under the lock (a few ms), write without it.
    let bytes = encode(
        &state.lock().unwrap_or_else(PoisonError::into_inner).history,
        now,
    );
    let target = path.to_owned();
    let result = tokio::task::spawn_blocking(move || write(&target, &bytes)).await;
    let err = match result {
        Ok(Ok(())) => return,
        Ok(Err(err)) => err.to_string(),
        Err(err) => err.to_string(),
    };
    let msg = format!(
        "fishnet-monitor: can't save the history to {}: {err}",
        path.display()
    );
    eprintln!("W: {msg}");
    state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .history
        .record_problem(now, ProblemLevel::W, &msg);
}

/// Saves every [`INTERVAL`]. Runs forever; spawn it as a task.
pub async fn run(path: PathBuf, state: Arc<Mutex<State>>) {
    let mut ticker = tokio::time::interval(INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // the first tick is immediate; nothing to save yet
    loop {
        ticker.tick().await;
        save(&path, &state).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Activity, DAY};

    fn at(s: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(s)
    }

    const T0: u64 = 1_795_219_200;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut h = History::new(at(T0));
        h.tick(at(T0), Activity::Busy);
        h.record_work(at(T0 + 10), 5, 50);
        h.record_problem(at(T0 + 10), ProblemLevel::E, "boom");

        write(&path, &encode(&h, at(T0 + 20))).unwrap();
        let back = load(&path, at(T0 + 20)).unwrap().unwrap();
        assert_eq!(back.minutes, h.minutes);
        assert_eq!(back.hours, h.hours);
        assert_eq!(back.problems, h.problems);
        assert_eq!(back.all, h.all);
        // No temporary file left behind.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn load_prunes_expired_buckets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut h = History::new(at(T0));
        h.record_work(at(T0), 5, 50);
        write(&path, &encode(&h, at(T0))).unwrap();
        let back = load(&path, at(T0 + 8 * DAY)).unwrap().unwrap();
        assert!(back.minutes.is_empty());
        assert_eq!(back.hours.len(), 1);
        assert_eq!(back.all.positions, 5);
    }

    #[test]
    fn missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("nope.json"), at(T0)), Ok(None));
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        fs::write(&path, b"{\"version\": 1, \"hist").unwrap();
        let err = load(&path, at(T0)).unwrap_err();
        assert!(err.contains("unreadable"), "{err}");
        assert!(err.contains("history.json.bad"), "{err}");
        assert!(!path.exists());
        assert!(dir.path().join("history.json.bad").exists());
    }

    #[test]
    fn unknown_version_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        fs::write(&path, br#"{"version": 99, "history": {}}"#).unwrap();
        let err = load(&path, at(T0)).unwrap_err();
        assert!(err.contains("version 99"), "{err}");
    }

    #[test]
    fn write_fails_cleanly_in_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing/history.json");
        assert!(write(&path, b"{}").is_err());
    }
}
