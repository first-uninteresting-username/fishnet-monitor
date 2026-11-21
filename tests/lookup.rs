//! Game lookup against a mock lichess export endpoint on 127.0.0.1, with
//! the games from `src/demo/games.ndjson`: lichess exports
//! (`/api/games/export/_ids`, NDJSON), also used by `fishnet-monitor demo`.
//! The first five are the named cases below; the rest are generated, with
//! made-up names.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use axum::{
    Router,
    extract::{RawQuery, State as Extract},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use fishnet_monitor::{
    history::{LookupMark, Player, ProblemLevel},
    lookup::{self, LookupState, Queue, Source},
    parser::parse_line,
    state::State,
    view::Changes,
};

const GAMES: &str = include_str!("../src/demo/games.ndjson");
/// Fixture ids for the cases in `games.ndjson`.
const TITLED_BOTH: &str = "HocfCBeq";
const BOT: &str = "NdMNAdpc";
const VS_AI: &str = "hMuL5Umg";
const ANONYMOUS: &str = "eMdQoGUK";
const SUPPORTERS: &str = "fMuJG9wX";

#[derive(Clone, Copy)]
enum Reply {
    Games,
    Status(u16),
}

#[derive(Debug, Clone)]
struct Request {
    query: String,
    accept: String,
    body: String,
}

#[derive(Clone)]
struct Mock {
    games: Arc<HashMap<String, String>>,
    replies: Arc<Mutex<Vec<Reply>>>,
    requests: Arc<Mutex<Vec<Request>>>,
}

async fn export(
    Extract(mock): Extract<Mock>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, String) {
    mock.requests.lock().unwrap().push(Request {
        query: query.unwrap_or_default(),
        accept: headers
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned(),
        body: body.clone(),
    });
    let reply = {
        let mut replies = mock.replies.lock().unwrap();
        if replies.len() > 1 {
            replies.remove(0)
        } else {
            replies[0]
        }
    };
    match reply {
        Reply::Status(code) => (StatusCode::from_u16(code).unwrap(), String::new()),
        Reply::Games => {
            let lines: Vec<&str> = body
                .split(',')
                .filter_map(|id| mock.games.get(id).map(String::as_str))
                .collect();
            (StatusCode::OK, lines.join("\n") + "\n")
        }
    }
}

/// Starts the mock and returns its base URL.
async fn start(replies: Vec<Reply>) -> (String, Mock) {
    let games = GAMES
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            (v["id"].as_str().unwrap().to_owned(), l.to_owned())
        })
        .collect();
    let mock = Mock {
        games: Arc::new(games),
        replies: Arc::new(Mutex::new(replies)),
        requests: Arc::default(),
    };
    let app = Router::new()
        .route("/api/games/export/_ids", post(export))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, mock)
}

fn t(s: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_219_200 + s)
}

/// A state with the lookup on, pointed at `base`, and the batch lines of
/// `urls` fed at t(0).
fn state_with(base: &str, urls: &[String]) -> Mutex<State> {
    let mut state = State::new(t(0));
    state.lookup = Queue::new(true, base);
    for url in urls {
        let line = format!("[    |            ] {url} finished (1500 knps/core)");
        state.apply_line(&parse_line(&line), t(0));
    }
    Mutex::new(state)
}

fn on(base: &str, ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| format!("{base}/{id}")).collect()
}

#[tokio::test]
async fn looks_up_games_and_counts_players() {
    let (base, mock) = start(vec![Reply::Games]).await;
    let mut urls = on(
        &base,
        &[TITLED_BOTH, BOT, VS_AI, ANONYMOUS, SUPPORTERS, "Gone1234"],
    );
    urls.push("https://other.example/AbCd1234".into());
    let state = state_with(&base, &urls);
    let changes = Changes::default();
    let source = Source::http(&base);

    assert!(
        !lookup::step(&state, &changes, &source, t(30)).await,
        "waits 60 s"
    );
    assert!(lookup::step(&state, &changes, &source, t(60)).await);

    let requests = mock.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].query.contains("moves=false"),
        "{:?}",
        requests[0]
    );
    assert_eq!(requests[0].accept, "application/x-ndjson");
    assert_eq!(
        requests[0].body,
        [TITLED_BOTH, BOT, VS_AI, ANONYMOUS, SUPPORTERS, "Gone1234"].join(","),
        "the other host's game isn't sent"
    );

    let s = state.lock().unwrap();
    let all = &s.history.all;
    assert_eq!((all.looked, all.missing), (5, 1));
    assert_eq!(all.titled, 1, "GM vs IM is one titled game");
    assert_eq!((all.by_title["GM"], all.by_title["IM"]), (1, 1));
    assert_eq!(all.bots, 1);
    assert!(!all.by_title.contains_key("BOT"));
    assert_eq!(all.patron, 2);
    let ranked: Vec<&str> = all.top.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(
        ranked,
        [TITLED_BOTH, BOT, SUPPORTERS],
        "no AI or unrated games"
    );
    assert_eq!(all.top[0].rating, Some(2684));

    let mark = |id: &str| s.history.batches.iter().find(|b| b.id == id).unwrap();
    assert_eq!(mark("Gone1234").lookup, LookupMark::Missing);
    assert_eq!(mark("AbCd1234").lookup, LookupMark::Skipped);
    let ai = mark(VS_AI);
    assert_eq!(ai.lookup, LookupMark::Done);
    assert_eq!(ai.game.as_ref().unwrap().black, Player::Ai { ai: 6 });

    let status = s.lookup_status();
    assert_eq!(
        (status.state, status.queued, status.last_count),
        (LookupState::Ok, 0, 6)
    );
    assert_eq!(s.lookup.counts.ok, 5);
}

#[tokio::test]
async fn rate_limit_waits_a_minute_and_keeps_the_ids() {
    let (base, mock) = start(vec![Reply::Status(429), Reply::Games]).await;
    let state = state_with(&base, &on(&base, &[TITLED_BOTH, BOT]));
    let changes = Changes::default();
    let source = Source::http(&base);

    assert!(lookup::step(&state, &changes, &source, t(60)).await);
    {
        let s = state.lock().unwrap();
        let status = s.lookup_status();
        assert_eq!(status.state, LookupState::Ratelimited);
        assert_eq!(status.queued, 2);
        assert_eq!(
            status.retry_at,
            Some(fishnet_monitor::history::millis(t(120)))
        );
        assert_eq!(s.lookup.counts.ratelimited, 1);
    }
    assert!(!lookup::step(&state, &changes, &source, t(119)).await);
    assert!(lookup::step(&state, &changes, &source, t(120)).await);
    assert_eq!(
        mock.requests.lock().unwrap()[1].body,
        [TITLED_BOTH, BOT].join(",")
    );
    let s = state.lock().unwrap();
    assert_eq!(s.lookup_status().state, LookupState::Ok);
    assert_eq!(s.history.all.looked, 2);
}

#[tokio::test]
async fn failed_request_is_retried_and_reported() {
    let (base, _mock) = start(vec![Reply::Status(502), Reply::Games]).await;
    let state = state_with(&base, &on(&base, &[TITLED_BOTH]));
    let changes = Changes::default();
    let source = Source::http(&base);

    assert!(lookup::step(&state, &changes, &source, t(60)).await);
    {
        let s = state.lock().unwrap();
        assert_eq!(s.lookup_status().state, LookupState::Error);
        assert_eq!(s.lookup.counts.error, 1);
        let p = &s.history.problems[0];
        assert_eq!(p.lvl, ProblemLevel::W);
        assert_eq!(p.msg, "fishnet-monitor: game lookup failed: HTTP 502");
        assert_eq!(s.history.batches[0].lookup, LookupMark::Pending);
    }
    assert!(lookup::step(&state, &changes, &source, t(120)).await);
    assert_eq!(
        state.lock().unwrap().history.batches[0].lookup,
        LookupMark::Done
    );
}

#[tokio::test]
async fn unreachable_api_is_an_error_without_the_url() {
    // Bind and drop to get a closed port.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let base = format!("http://127.0.0.1:{port}");
    let state = state_with(&base, &on(&base, &[TITLED_BOTH]));
    assert!(lookup::step(&state, &Changes::default(), &Source::http(&base), t(60)).await);
    let s = state.lock().unwrap();
    assert_eq!(s.lookup_status().state, LookupState::Error);
    assert!(!s.history.problems[0].msg.contains("127.0.0.1"));
}

#[tokio::test]
async fn three_hundred_waiting_go_out_at_once() {
    let (base, mock) = start(vec![Reply::Games]).await;
    let ids: Vec<String> = (0..lookup::BATCH + 20).map(|i| format!("g{i}")).collect();
    let urls: Vec<String> = ids.iter().map(|id| format!("{base}/{id}")).collect();
    let state = state_with(&base, &urls);
    let changes = Changes::default();
    let source = Source::http(&base);

    assert!(lookup::step(&state, &changes, &source, t(1)).await);
    assert_eq!(
        mock.requests.lock().unwrap()[0].body.split(',').count(),
        lookup::BATCH
    );
    let s = state.lock().unwrap();
    assert_eq!(s.history.all.missing, lookup::BATCH as u64);
    assert_eq!(s.lookup_status().queued, 20);
}

#[tokio::test]
async fn fixture_source_needs_no_network() {
    let source = Source::fixture(GAMES);
    let games = source
        .fetch(&[TITLED_BOTH.to_owned(), "nope".to_owned()])
        .await
        .unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].id, TITLED_BOTH);
}
