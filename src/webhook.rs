//! Webhook notifications for `fatal` and `stall` (see "Webhooks" in
//! `PLAN.md`).

use std::{sync::Arc, time::Duration};

use serde::Serialize;
use tokio::{sync::mpsc, task::JoinSet};

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
        Payload {
            event: alert.kind.as_str().to_owned(),
            time: humantime::format_rfc3339_seconds(alert.time).to_string(),
            host: host.to_owned(),
            state: alert.state.clone(),
            message: alert.message.clone(),
        }
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
    // Try once, then once more after each wait. The waits run out after the
    // last retry, and that attempt's error is the one returned.
    let mut waits = delivery.backoff.iter();
    loop {
        let err = match attempt(client, url, payload, delivery.timeout).await {
            Ok(()) => return Ok(()),
            Err(err) => err,
        };
        match waits.next() {
            Some(wait) => tokio::time::sleep(*wait).await,
            None => return Err(err),
        }
    }
}

async fn attempt(
    client: &reqwest::Client,
    url: &str,
    payload: &Payload,
    timeout: Duration,
) -> Result<(), String> {
    let response = client
        .post(url)
        .json(payload)
        .timeout(timeout)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status);
    match response {
        Ok(_) => Ok(()),
        // Report the status code only. The default message includes the URL,
        // which can carry a secret (Slack-style webhook tokens).
        Err(err) => Err(match err.status() {
            Some(status) => format!("HTTP {}", status.as_u16()),
            None => err.without_url().to_string(),
        }),
    }
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
    // One client so connections are pooled across alerts.
    let client = reqwest::Client::new();
    let url = Arc::new(url);
    let host = Arc::new(host);
    let delivery = Arc::new(delivery);
    let on_failure = Arc::new(on_failure);

    let mut tasks = JoinSet::new();
    while let Some(alert) = alerts.recv().await {
        let client = client.clone();
        let url = Arc::clone(&url);
        let host = Arc::clone(&host);
        let delivery = Arc::clone(&delivery);
        let on_failure = Arc::clone(&on_failure);
        tasks.spawn(async move {
            let payload = Payload::new(&alert, &host);
            if let Err(err) = send(&client, &url, &payload, &delivery).await {
                on_failure(format!("webhook {} failed: {err}", payload.event));
            }
        });
    }
    // The channel is closed. Wait for the in-flight deliveries so the caller
    // (and tests) see their effects before `run` returns.
    tasks.join_all().await;
}
