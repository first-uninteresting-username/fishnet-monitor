//! The HTTP server: `/api/status`, `/metrics`, `/healthz`. Phase 3 adds the
//! dashboard, `/api/history` and `/api/events`.

use std::{
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime},
};

use axum::{
    Json, Router,
    extract::State as Extract,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{
    metrics,
    state::{State, StatusContext},
};

/// Timeout for each step of [`healthcheck`].
const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct AppState {
    pub state: Arc<Mutex<State>>,
    pub ctx: Arc<StatusContext>,
}

/// Routes:
/// - `GET /api/status`: `State::status` as JSON.
/// - `GET /metrics`: `metrics::render`, with `metrics::CONTENT_TYPE`.
/// - `GET /healthz`: 200 `ok\n` when healthy, else 503 with the reasons,
///   one per line.
pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/api/status", get(api_status))
        .route("/metrics", get(metrics_endpoint))
        .route("/healthz", get(healthz))
        .with_state(app)
}

/// Binds `addr` and serves [`router`] until the future is dropped.
pub async fn serve(addr: SocketAddr, app: AppState) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(app)).await
}

/// `fishnet-monitor healthcheck`: `GET http://<addr>/healthz` over a plain
/// `std::net::TcpStream` (no HTTP client crate, 3 s timeouts). A `0.0.0.0`
/// or `[::]` listen address is replaced by loopback. Returns true on 200.
pub fn healthcheck(addr: SocketAddr) -> bool {
    let target = loopback_if_unspecified(addr);
    let Ok(mut stream) = TcpStream::connect_timeout(&target, HEALTHCHECK_TIMEOUT) else {
        return false;
    };
    let timeouts = stream
        .set_read_timeout(Some(HEALTHCHECK_TIMEOUT))
        .and(stream.set_write_timeout(Some(HEALTHCHECK_TIMEOUT)));
    let request = "GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if timeouts.is_err() || stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    // Only the status line matters, e.g. "HTTP/1.1 200 OK".
    let mut status_line = String::new();
    if BufReader::new(stream).read_line(&mut status_line).is_err() {
        return false;
    }
    status_line.split_whitespace().nth(1) == Some("200")
}

/// Maps `0.0.0.0` and `[::]` to the matching loopback address; other
/// addresses are returned unchanged.
fn loopback_if_unspecified(addr: SocketAddr) -> SocketAddr {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => other,
    };
    SocketAddr::new(ip, addr.port())
}

/// Locks the shared state. A poisoned lock (a panic while it was held) is
/// recovered rather than propagated, so the monitor keeps answering.
fn lock(app: &AppState) -> MutexGuard<'_, State> {
    app.state.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn api_status(Extract(app): Extract<AppState>) -> Response {
    let now = SystemTime::now();
    let status = lock(&app).status(now, &app.ctx);
    ([(header::CACHE_CONTROL, "no-store")], Json(status)).into_response()
}

async fn metrics_endpoint(Extract(app): Extract<AppState>) -> Response {
    let now = SystemTime::now();
    let body = metrics::render(&lock(&app), now, app.ctx.stall_timeout);
    ([(header::CONTENT_TYPE, metrics::CONTENT_TYPE)], body).into_response()
}

async fn healthz(Extract(app): Extract<AppState>) -> Response {
    let now = SystemTime::now();
    let health = lock(&app).health(now, app.ctx.stall_timeout);
    if health.healthy {
        (StatusCode::OK, "ok\n").into_response()
    } else {
        let mut body = health.reasons.join("\n");
        body.push('\n');
        (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
        net::{Ipv4Addr, SocketAddr, TcpListener},
        sync::mpsc,
        thread,
    };

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// Serves one connection with `status_line`, reporting the request head
    /// (request line and headers) through the returned channel.
    fn fake_healthz_server(status_line: &'static str) -> (SocketAddr, mpsc::Receiver<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                head.push_str(&line);
            }
            // The receiver may already be gone; the reply still matters.
            let _ = tx.send(head);
            let response =
                format!("{status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            stream.write_all(response.as_bytes()).unwrap();
        });
        (addr, rx)
    }

    #[test]
    fn healthcheck_is_true_on_200_and_sends_healthz_request() {
        let (addr, head) = fake_healthz_server("HTTP/1.1 200 OK");
        assert!(healthcheck(addr));
        let head = head.recv().unwrap();
        assert!(head.starts_with("GET /healthz HTTP/1.1\r\n"), "{head:?}");
        assert!(head.contains("Host: localhost\r\n"), "{head:?}");
        assert!(head.contains("Connection: close\r\n"), "{head:?}");
    }

    #[test]
    fn healthcheck_is_false_on_503() {
        let (addr, _) = fake_healthz_server("HTTP/1.1 503 Service Unavailable");
        assert!(!healthcheck(addr));
    }

    #[test]
    fn healthcheck_replaces_unspecified_address_with_loopback() {
        let (addr, _) = fake_healthz_server("HTTP/1.1 200 OK");
        let any = SocketAddr::from(([0, 0, 0, 0], addr.port()));
        assert!(healthcheck(any));
    }

    #[test]
    fn healthcheck_is_false_when_nothing_listens() {
        // Bind to get a free port, then close it again.
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert!(!healthcheck(SocketAddr::from((Ipv4Addr::LOCALHOST, port))));
    }

    #[test]
    fn loopback_mapping_keeps_specific_addresses() {
        let specific: SocketAddr = "192.168.1.5:9000".parse().unwrap();
        assert_eq!(loopback_if_unspecified(specific), specific);
        let v6_any: SocketAddr = "[::]:9000".parse().unwrap();
        assert_eq!(
            loopback_if_unspecified(v6_any),
            "[::1]:9000".parse().unwrap()
        );
    }

    fn test_app() -> AppState {
        AppState {
            state: Arc::new(Mutex::new(State::new(SystemTime::now()))),
            ctx: Arc::new(StatusContext {
                host: "test-host".to_owned(),
                listen: "127.0.0.1:0".to_owned(),
                stall_timeout: Duration::from_secs(900),
            }),
        }
    }

    /// Starts `router(app)` on an ephemeral port and returns its address.
    async fn spawn_server(app: AppState) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router(app)).await.unwrap();
        });
        addr
    }

    /// Sends a raw request (with `Connection: close`) and returns the response.
    async fn raw_request(addr: SocketAddr, method: &str, path: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    // The handlers for the three GET routes call State::status/health and
    // render, which are still todo!(), so these tests only use paths that
    // never reach a handler body.

    #[tokio::test]
    async fn unknown_path_is_404() {
        let addr = spawn_server(test_app()).await;
        let response = raw_request(addr, "GET", "/nope").await;
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }

    #[tokio::test]
    async fn known_paths_reject_other_methods_with_405() {
        let addr = spawn_server(test_app()).await;
        for path in ["/api/status", "/metrics", "/healthz"] {
            let response = raw_request(addr, "POST", path).await;
            assert!(response.starts_with("HTTP/1.1 405"), "{path}: {response}");
        }
    }
}
