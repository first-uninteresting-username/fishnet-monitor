//! Reads fishnet's stats file (`stats.rs` upstream): JSON
//! `{"total_batches": N, "total_positions": N, "total_nodes": N}`, rewritten
//! after every finished batch.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use serde::Deserialize;

use crate::state::{State, Totals};

/// The file's JSON shape. Serde ignores fields not listed here.
#[derive(Deserialize)]
struct StatsFile {
    total_batches: u64,
    total_positions: u64,
    total_nodes: u64,
}

/// Parses the file content. Extra fields are ignored; missing counters or
/// invalid JSON are `None`.
pub fn parse(content: &str) -> Option<Totals> {
    let file: StatsFile = serde_json::from_str(content).ok()?;
    Some(Totals {
        batches: file.total_batches,
        positions: file.total_positions,
        nodes: file.total_nodes,
    })
}

/// Polls `path` every `interval` and calls [`State::apply_stats_file`]
/// (then `on_change`) whenever the content parses and differs from the last
/// value applied.
/// A missing or half-written file is skipped silently (fishnet creates it
/// after the first batch). Runs forever; spawn it as a task.
pub async fn watch(
    path: PathBuf,
    interval: Duration,
    state: Arc<Mutex<State>>,
    on_change: impl Fn(),
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last = None;
    loop {
        ticker.tick().await;
        if let Some(totals) = poll_once(&path, &mut last).await {
            // Lock only for the synchronous call; never hold it across an await.
            state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .apply_stats_file(totals, std::time::SystemTime::now());
            on_change();
        }
    }
}

/// One poll step: reads and parses `path`, and returns the value to apply
/// if it is new. `last` tracks the most recent value returned, so unchanged
/// content gives `None`. A failed read or parse leaves `last` alone, so a
/// half-written file does not cause a spurious re-apply later.
async fn poll_once(path: &Path, last: &mut Option<Totals>) -> Option<Totals> {
    let content = tokio::fs::read_to_string(path).await.ok()?;
    let totals = parse(&content)?;
    if *last == Some(totals) {
        return None;
    }
    *last = Some(totals);
    Some(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"total_batches": 3, "total_positions": 300, "total_nodes": 4000000}"#;

    fn totals(batches: u64, positions: u64, nodes: u64) -> Totals {
        Totals {
            batches,
            positions,
            nodes,
        }
    }

    #[test]
    fn parse_reads_all_counters() {
        assert_eq!(parse(VALID), Some(totals(3, 300, 4_000_000)));
    }

    #[test]
    fn parse_ignores_extra_fields() {
        let content =
            r#"{"total_batches": 1, "total_positions": 2, "total_nodes": 3, "other": true}"#;
        assert_eq!(parse(content), Some(totals(1, 2, 3)));
    }

    #[test]
    fn parse_rejects_missing_field() {
        assert_eq!(parse(r#"{"total_batches": 1, "total_positions": 2}"#), None);
    }

    #[test]
    fn parse_rejects_invalid_json() {
        assert_eq!(parse(r#"{"total_batches": 1, "total_pos"#), None);
        assert_eq!(parse(""), None);
    }

    #[tokio::test]
    async fn poll_once_missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut last = None;

        assert_eq!(
            poll_once(&dir.path().join("stats.json"), &mut last).await,
            None
        );
        assert_eq!(last, None);
    }

    #[tokio::test]
    async fn poll_once_returns_new_value_then_none_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, VALID).unwrap();
        let mut last = None;

        assert_eq!(
            poll_once(&path, &mut last).await,
            Some(totals(3, 300, 4_000_000))
        );
        assert_eq!(poll_once(&path, &mut last).await, None);
    }

    #[tokio::test]
    async fn poll_once_returns_value_again_after_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, VALID).unwrap();
        let mut last = None;
        poll_once(&path, &mut last).await;

        std::fs::write(
            &path,
            r#"{"total_batches": 4, "total_positions": 400, "total_nodes": 5}"#,
        )
        .unwrap();

        assert_eq!(poll_once(&path, &mut last).await, Some(totals(4, 400, 5)));
        assert_eq!(last, Some(totals(4, 400, 5)));
    }

    #[tokio::test]
    async fn poll_once_skips_half_written_file_and_keeps_last() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");
        std::fs::write(&path, VALID).unwrap();
        let mut last = None;
        poll_once(&path, &mut last).await;

        std::fs::write(&path, r#"{"total_batches": 9, "total_pos"#).unwrap();
        assert_eq!(poll_once(&path, &mut last).await, None);
        assert_eq!(last, Some(totals(3, 300, 4_000_000)));

        // The file is finished with the same content as before: still nothing new.
        std::fs::write(&path, VALID).unwrap();
        assert_eq!(poll_once(&path, &mut last).await, None);
    }
}
