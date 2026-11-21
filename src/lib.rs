//! fishnet-monitor: runs the lichess fishnet client and reports what it is
//! doing. See `PLAN.md` for the overall design.

pub mod config;
pub mod http;
pub mod metrics;
pub mod parser;
pub mod state;
pub mod stats_file;
pub mod supervisor;
pub mod webhook;
