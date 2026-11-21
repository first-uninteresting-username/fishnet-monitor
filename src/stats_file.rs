//! Reads fishnet's stats file (`stats.rs` upstream): JSON
//! `{"total_batches": N, "total_positions": N, "total_nodes": N}`, rewritten
//! after every finished batch.

use std::{path::PathBuf, sync::Arc, sync::Mutex, time::Duration};

use crate::state::{State, Totals};

/// Parses the file content. Extra fields are ignored; missing counters or
/// invalid JSON are `None`.
pub fn parse(content: &str) -> Option<Totals> {
    let _ = content;
    todo!("fragment: stats_file")
}

/// Polls `path` every `interval` and calls [`State::apply_stats_file`]
/// whenever the content parses and differs from the last value applied.
/// A missing or half-written file is skipped silently (fishnet creates it
/// after the first batch). Runs forever; spawn it as a task.
pub async fn watch(path: PathBuf, interval: Duration, state: Arc<Mutex<State>>) {
    let _ = (path, interval, state);
    todo!("fragment: stats_file")
}
