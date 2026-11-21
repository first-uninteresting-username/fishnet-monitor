//! Webhook delivery against a mock HTTP receiver on 127.0.0.1.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use fishnet_monitor::{
    state::{Alert, AlertKind},
    webhook::{Delivery, Payload, run, send},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

/// How the mock answers the n-th request. The last entry repeats once the
/// list runs out, so `[Status(500)]` means "always 500".
#[derive(Clone, Copy)]
enum Reply {
    Status(u16),
    /// Sleeps this long, then answers 200. Longer than the client timeout
    /// makes the client give up first.
    Slow(Duration),
}

#[derive(Debug, Clone)]
struct Request {
    content_type: Option<String>,
    body: Vec<u8>,
}

struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Mock {
    async fn start(replies: Vec<Reply>) -> Mock {
        assert!(!replies.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let shared = Arc::clone(&shared);
                let replies = replies.clone();
                tokio::spawn(serve(stream, shared, replies));
            }
        });
        Mock { url, requests }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

/// Reads one HTTP/1.1 request (headers up to the blank line, then
/// `Content-Length` bytes of body), records it and writes the reply.
async fn serve(stream: TcpStream, requests: Arc<Mutex<Vec<Request>>>, replies: Vec<Reply>) {
    let mut reader = BufReader::new(stream);
    let mut content_type = None;
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-type" => content_type = Some(value.trim().to_owned()),
                "content-length" => length = value.trim().parse().unwrap(),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.unwrap();

    let index = {
        let mut recorded = requests.lock().unwrap();
        recorded.push(Request { content_type, body });
        recorded.len() - 1
    };
    let status = match replies[index.min(replies.len() - 1)] {
        Reply::Status(code) => code,
        Reply::Slow(delay) => {
            tokio::time::sleep(delay).await;
            200
        }
    };

    let response =
        format!("HTTP/1.1 {status} Mock\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let stream = reader.get_mut();
    // The client may already have given up (timeout); a failed write is fine.
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Short schedule so the retry tests finish in well under a second.
fn quick() -> Delivery {
    Delivery {
        timeout: Duration::from_millis(200),
        backoff: vec![Duration::from_millis(10); 3],
    }
}

/// `UNIX_EPOCH + 1000 s`, which formats as `1970-01-01T00:16:40Z`.
fn alert(kind: AlertKind, message: &str) -> Alert {
    Alert {
        kind,
        message: message.to_owned(),
        state: "running".to_owned(),
        time: SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
    }
}

fn parse_body(request: &Request) -> serde_json::Value {
    serde_json::from_slice(&request.body).expect("body is JSON")
}

#[test]
fn payload_fields_come_from_the_alert() {
    let payload = Payload::new(&alert(AlertKind::Stall, "no batches"), "box-1");
    assert_eq!(payload.event, "stall");
    assert_eq!(payload.time, "1970-01-01T00:16:40Z");
    assert_eq!(payload.host, "box-1");
    assert_eq!(payload.state, "running");
    assert_eq!(payload.message, "no batches");
}

/// The fishnet key can only reach the payload through `message`, which is a
/// fishnet log line. fishnet never prints its key, so the body keeps exactly
/// these five keys and nothing else.
#[test]
fn payload_has_exactly_five_keys() {
    let payload = Payload::new(&alert(AlertKind::Fatal, "E: Stopping queue."), "box-1");
    let json = serde_json::to_value(&payload).unwrap();
    let keys: BTreeSet<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from(["event", "host", "message", "state", "time"])
    );
}

#[tokio::test]
async fn post_is_json_with_the_payload_fields() {
    let mock = Mock::start(vec![Reply::Status(200)]).await;
    let payload = Payload::new(&alert(AlertKind::Fatal, "E: boom"), "box-1");

    send(&reqwest::Client::new(), &mock.url, &payload, &quick())
        .await
        .unwrap();

    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some("application/json")
    );
    assert_eq!(
        parse_body(&requests[0]),
        serde_json::json!({
            "event": "fatal",
            "time": "1970-01-01T00:16:40Z",
            "host": "box-1",
            "state": "running",
            "message": "E: boom",
        })
    );
}

#[tokio::test]
async fn retries_until_the_receiver_accepts() {
    let mock = Mock::start(vec![
        Reply::Status(500),
        Reply::Status(500),
        Reply::Status(200),
    ])
    .await;
    let payload = Payload::new(&alert(AlertKind::Stall, "stalled"), "box-1");

    let result = send(&reqwest::Client::new(), &mock.url, &payload, &quick()).await;

    assert_eq!(result, Ok(()));
    assert_eq!(mock.requests().len(), 3);
}

#[tokio::test]
async fn gives_up_after_four_attempts() {
    let mock = Mock::start(vec![Reply::Status(500)]).await;
    let payload = Payload::new(&alert(AlertKind::Stall, "stalled"), "box-1");

    let result = send(&reqwest::Client::new(), &mock.url, &payload, &quick()).await;

    let err = result.unwrap_err();
    assert!(err.contains("HTTP 500"), "unexpected error: {err}");
    assert_eq!(mock.requests().len(), 4);
}

#[tokio::test]
async fn slow_receiver_counts_as_failure() {
    let mock = Mock::start(vec![Reply::Slow(Duration::from_secs(2))]).await;
    let payload = Payload::new(&alert(AlertKind::Stall, "stalled"), "box-1");

    let result = send(&reqwest::Client::new(), &mock.url, &payload, &quick()).await;

    assert!(result.is_err());
    // The server still records each attempt when it reads it, before it sleeps.
    assert_eq!(mock.requests().len(), 4);
}

#[tokio::test]
async fn run_delivers_each_alert() {
    let mock = Mock::start(vec![Reply::Status(200)]).await;
    let (tx, rx) = mpsc::channel(8);
    tx.send(alert(AlertKind::Fatal, "E: one")).await.unwrap();
    tx.send(alert(AlertKind::Stall, "two")).await.unwrap();
    drop(tx);

    tokio::time::timeout(
        Duration::from_secs(5),
        run(mock.url.clone(), "box-1".into(), quick(), rx, |err| {
            panic!("unexpected failure: {err}")
        }),
    )
    .await
    .expect("run returns once the sender is dropped");

    let mut events: Vec<String> = mock
        .requests()
        .iter()
        .map(|r| parse_body(r)["event"].as_str().unwrap().to_owned())
        .collect();
    events.sort();
    assert_eq!(events, ["fatal", "stall"]);
}

#[tokio::test]
async fn run_reports_each_final_failure() {
    let mock = Mock::start(vec![Reply::Status(500)]).await;
    let (tx, rx) = mpsc::channel(8);
    tx.send(alert(AlertKind::Fatal, "E: one")).await.unwrap();
    tx.send(alert(AlertKind::Stall, "two")).await.unwrap();
    drop(tx);

    let failures = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&failures);
    tokio::time::timeout(
        Duration::from_secs(5),
        run(mock.url.clone(), "box-1".into(), quick(), rx, move |err| {
            sink.lock().unwrap().push(err)
        }),
    )
    .await
    .expect("run returns once the sender is dropped");

    // Each alert gets 4 attempts. Deliveries run concurrently, so sort the
    // reports before comparing.
    assert_eq!(mock.requests().len(), 8);
    let mut failures = failures.lock().unwrap().clone();
    failures.sort();
    assert_eq!(
        failures,
        [
            "webhook fatal failed: HTTP 500",
            "webhook stall failed: HTTP 500",
        ]
    );
}

#[tokio::test]
async fn nothing_sent_means_no_request() {
    let mock = Mock::start(vec![Reply::Status(200)]).await;
    let (tx, rx) = mpsc::channel::<Alert>(8);
    drop(tx);

    run(mock.url.clone(), "box-1".into(), quick(), rx, |err| {
        panic!("unexpected failure: {err}")
    })
    .await;

    assert!(mock.requests().is_empty());
}
