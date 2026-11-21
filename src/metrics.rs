//! `/metrics` in the Prometheus text exposition format (version 0.0.4),
//! rendered by hand from [`State`].

use std::time::{Duration, SystemTime};

use crate::state::State;

/// Content type of [`render`]'s output.
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Renders all metrics from the table in `PLAN.md` that phase 2 has data
/// for (everything except the game-lookup metrics).
pub fn render(state: &State, now: SystemTime, stall_timeout: Duration) -> String {
    let _ = (state, now, stall_timeout);
    todo!("fragment: http")
}
