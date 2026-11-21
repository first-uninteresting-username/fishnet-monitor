//! Settings from environment variables (see `PLAN.md`).

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

/// Default for `MONITOR_LISTEN`.
const DEFAULT_LISTEN: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 9465));
/// Default for `STALL_TIMEOUT`.
const DEFAULT_STALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

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
    /// `HISTORY_FILE`; unset or empty keeps the history in memory only.
    pub history_file: Option<PathBuf>,
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
        // Empty values behave exactly like unset ones.
        let get = |name: &str| lookup(name).filter(|value| !value.is_empty());

        let listen = match get("MONITOR_LISTEN") {
            Some(value) => value
                .parse()
                .map_err(|_| format!("MONITOR_LISTEN: invalid address \"{value}\""))?,
            None => DEFAULT_LISTEN,
        };

        let stall_timeout = match get("STALL_TIMEOUT") {
            Some(value) => parse_stall_timeout(&value)?,
            None => DEFAULT_STALL_TIMEOUT,
        };

        let forward_debug = match get("FORWARD_DEBUG") {
            Some(value) => parse_bool("FORWARD_DEBUG", &value)?,
            None => false,
        };

        let host_name = get("MONITOR_HOST_NAME")
            .or_else(|| get("HOSTNAME"))
            .or_else(system_host_name)
            .unwrap_or_else(|| "unknown".to_string());

        Ok(Config {
            listen,
            stall_timeout,
            stats_file: get("STATS_FILE").map(PathBuf::from),
            history_file: get("HISTORY_FILE").map(PathBuf::from),
            forward_debug,
            webhook_url: get("WEBHOOK_URL"),
            host_name,
        })
    }
}

/// A bare integer means seconds; anything else goes through humantime.
/// Zero is rejected because a zero timeout would report a stall at once.
fn parse_stall_timeout(value: &str) -> Result<Duration, String> {
    let invalid = || format!("STALL_TIMEOUT: invalid duration \"{value}\"");
    let duration = if value.bytes().all(|b| b.is_ascii_digit()) {
        value
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| invalid())?
    } else {
        humantime::parse_duration(value).map_err(|_| invalid())?
    };
    if duration.is_zero() {
        return Err(format!(
            "STALL_TIMEOUT: must be greater than zero \"{value}\""
        ));
    }
    Ok(duration)
}

fn parse_bool(name: &str, value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("{name}: invalid boolean \"{value}\"")),
    }
}

/// The machine's hostname from `/etc/hostname`. Read errors and blank files
/// count as "no hostname", so the caller falls back to `"unknown"`.
fn system_host_name() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Loads a config from fixed variable pairs, standing in for the real
    /// environment.
    fn load(pairs: &[(&str, &str)]) -> Result<Config, String> {
        let vars: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        Config::from_lookup(move |name| vars.get(name).cloned())
    }

    #[test]
    fn defaults_when_nothing_is_set() {
        let config = load(&[]).unwrap();

        assert_eq!(config.listen, "0.0.0.0:9465".parse().unwrap());
        assert_eq!(config.stall_timeout, Duration::from_secs(15 * 60));
        assert_eq!(config.stats_file, None);
        assert_eq!(config.history_file, None);
        assert!(!config.forward_debug);
        assert_eq!(config.webhook_url, None);
        // The fallback chain ends in "unknown", so this is never empty.
        assert!(!config.host_name.is_empty());
    }

    #[test]
    fn every_variable_set() {
        let config = load(&[
            ("MONITOR_LISTEN", "127.0.0.1:1234"),
            ("STALL_TIMEOUT", "90s"),
            ("STATS_FILE", "/var/lib/fishnet/stats.json"),
            ("FORWARD_DEBUG", "yes"),
            ("HISTORY_FILE", "/data/history.json"),
            ("WEBHOOK_URL", "https://example.com/hook"),
            ("MONITOR_HOST_NAME", "box"),
            ("HOSTNAME", "ignored"),
        ])
        .unwrap();

        assert_eq!(config.listen, "127.0.0.1:1234".parse().unwrap());
        assert_eq!(config.stall_timeout, Duration::from_secs(90));
        assert_eq!(
            config.stats_file,
            Some(PathBuf::from("/var/lib/fishnet/stats.json"))
        );
        assert!(config.forward_debug);
        assert_eq!(
            config.history_file,
            Some(PathBuf::from("/data/history.json"))
        );
        assert_eq!(
            config.webhook_url.as_deref(),
            Some("https://example.com/hook")
        );
        assert_eq!(config.host_name, "box");
    }

    #[test]
    fn stall_timeout_accepts_seconds_and_humantime() {
        for (input, seconds) in [("15m", 900), ("900", 900), ("1h 30m", 5400), ("90s", 90)] {
            let config = load(&[("STALL_TIMEOUT", input)]).unwrap();
            assert_eq!(
                config.stall_timeout,
                Duration::from_secs(seconds),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn stall_timeout_rejects_invalid_and_zero() {
        assert_eq!(
            load(&[("STALL_TIMEOUT", "abc")]),
            Err("STALL_TIMEOUT: invalid duration \"abc\"".to_string())
        );
        assert_eq!(
            load(&[("STALL_TIMEOUT", "0")]),
            Err("STALL_TIMEOUT: must be greater than zero \"0\"".to_string())
        );
        assert_eq!(
            load(&[("STALL_TIMEOUT", "0s")]),
            Err("STALL_TIMEOUT: must be greater than zero \"0s\"".to_string())
        );
    }

    #[test]
    fn invalid_listen_address_is_an_error() {
        assert_eq!(
            load(&[("MONITOR_LISTEN", "nope")]),
            Err("MONITOR_LISTEN: invalid address \"nope\"".to_string())
        );
    }

    #[test]
    fn booleans_accept_common_spellings() {
        for value in ["1", "true", "YES", "On"] {
            assert!(
                load(&[("FORWARD_DEBUG", value)]).unwrap().forward_debug,
                "{value}"
            );
        }
        for value in ["0", "False", "no", "OFF"] {
            assert!(
                !load(&[("FORWARD_DEBUG", value)]).unwrap().forward_debug,
                "{value}"
            );
        }
    }

    #[test]
    fn invalid_boolean_is_an_error() {
        assert_eq!(
            load(&[("FORWARD_DEBUG", "maybe")]),
            Err("FORWARD_DEBUG: invalid boolean \"maybe\"".to_string())
        );
    }

    #[test]
    fn empty_values_count_as_unset() {
        let empty = load(&[
            ("MONITOR_LISTEN", ""),
            ("STALL_TIMEOUT", ""),
            ("STATS_FILE", ""),
            ("FORWARD_DEBUG", ""),
            ("WEBHOOK_URL", ""),
            ("MONITOR_HOST_NAME", ""),
            ("HOSTNAME", ""),
        ]);
        // Empty numbers and booleans must not fail, so this is `Ok`.
        assert_eq!(empty, load(&[]));
    }

    #[test]
    fn host_name_prefers_monitor_host_name_then_hostname() {
        let both = load(&[("MONITOR_HOST_NAME", "monitor"), ("HOSTNAME", "system")]).unwrap();
        assert_eq!(both.host_name, "monitor");

        let only_hostname = load(&[("HOSTNAME", "system")]).unwrap();
        assert_eq!(only_hostname.host_name, "system");

        let empty_override = load(&[("MONITOR_HOST_NAME", ""), ("HOSTNAME", "system")]).unwrap();
        assert_eq!(empty_override.host_name, "system");
    }
}
