//! Webhook notifications for `fatal` and `stall` (see "Webhooks" in
//! `PLAN.md`).

use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::state::Alert;

/// The JSON body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payload {
    /// `"fatal"` or `"stall"`.
    pub event: String,
    /// RFC 3339, UTC, seconds precision (`humantime::format_rfc3339_seconds`).
    pub time: String,
    pub host: String,
    pub state: String,
    pub message: String,
}

impl Payload {
    pub fn new(alert: &Alert, host: &str) -> Payload {
        let _ = (alert, host);
        todo!("fragment: webhook")
    }
}

/// Delivery settings; [`Delivery::default`] is the production schedule.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub timeout: Duration,
    /// Waits before each retry; the number of retries is its length.
    pub backoff: Vec<Duration>,
}

impl Default for Delivery {
    fn default() -> Self {
        Delivery {
            timeout: Duration::from_secs(10),
            backoff: vec![
                Duration::from_secs(1),
                Duration::from_secs(5),
                Duration::from_secs(30),
            ],
        }
    }
}

/// POSTs one payload, retrying per `delivery`. A non-2xx status counts as a
/// failure. Returns the last error message after the final failure.
pub async fn send(
    client: &reqwest::Client,
    url: &str,
    payload: &Payload,
    delivery: &Delivery,
) -> Result<(), String> {
    let _ = (client, url, payload, delivery);
    todo!("fragment: webhook")
}

/// Receives alerts and delivers each one on its own task, so a slow
/// receiver never blocks the next alert. Final failures are passed to
/// `on_failure` (the caller turns them into a monitor `W:` line).
/// Runs until the channel closes.
pub async fn run(
    url: String,
    host: String,
    delivery: Delivery,
    mut alerts: mpsc::Receiver<Alert>,
    on_failure: impl Fn(String) + Send + Sync + 'static,
) {
    let _ = (&url, &host, &delivery, &mut alerts, on_failure);
    todo!("fragment: webhook")
}
