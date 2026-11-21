//! The HTTP server: `/api/status`, `/metrics`, `/healthz`. Phase 3 adds the
//! dashboard, `/api/history` and `/api/events`.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::Router;

use crate::state::{State, StatusContext};

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
    let _ = app;
    todo!("fragment: http")
}

/// Binds `addr` and serves [`router`] until the future is dropped.
pub async fn serve(addr: SocketAddr, app: AppState) -> std::io::Result<()> {
    let _ = (addr, app);
    todo!("fragment: http")
}

/// `fishnet-monitor healthcheck`: `GET http://<addr>/healthz` over a plain
/// `std::net::TcpStream` (no HTTP client crate, 3 s timeouts). A `0.0.0.0`
/// or `[::]` listen address is replaced by loopback. Returns true on 200.
pub fn healthcheck(addr: SocketAddr) -> bool {
    let _ = addr;
    todo!("fragment: http")
}
