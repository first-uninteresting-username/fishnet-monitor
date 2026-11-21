//! Settings from environment variables (see `PLAN.md`).

use std::{net::SocketAddr, path::PathBuf, time::Duration};

/// Everything the monitor reads from the environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `MONITOR_LISTEN`, default `0.0.0.0:9465`.
    pub listen: SocketAddr,
    /// `STALL_TIMEOUT`, default 15 min. Accepts humantime (`15m`, `90s`,
    /// `1h 30m`) or a bare number of seconds (`900`).
    pub stall_timeout: Duration,
    /// `STATS_FILE`; unset or empty means no stats file.
    pub stats_file: Option<PathBuf>,
    /// `FORWARD_DEBUG=1` forwards fishnet's `D:` lines. Default off.
    pub forward_debug: bool,
    /// `WEBHOOK_URL`; unset or empty disables webhooks.
    pub webhook_url: Option<String>,
    /// `MONITOR_HOST_NAME`, else `HOSTNAME`, else the content of
    /// `/etc/hostname` (trimmed), else `"unknown"`.
    pub host_name: String,
}

impl Config {
    /// Reads the real environment.
    pub fn from_env() -> Result<Config, String> {
        Config::from_lookup(|name| std::env::var(name).ok())
    }

    /// Builds the config from a variable lookup, so tests don't touch the
    /// process environment. Invalid values are an error naming the variable,
    /// for example `"STALL_TIMEOUT: invalid duration \"abc\""`.
    /// Boolean flags accept `1`/`true`/`yes`/`on` and `0`/`false`/`no`/`off`
    /// (case-insensitive); empty counts as unset.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let _ = lookup;
        todo!("fragment: config")
    }
}
