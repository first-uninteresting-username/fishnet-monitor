#!/usr/bin/env python3
"""A stand-in for lichess's fishnet work API, for scripts/smoke-test.sh.

It answers only what the fishnet client calls (src/api.rs in
lichess-org/fishnet): one small analysis job on the first acquire, then no
work. It also receives the monitor's webhooks. Test code only: the monitor
itself never talks to /fishnet/*.

    mock-fishnet.py --key KEY --dir DIR

Listens on 0.0.0.0 on a free port and writes the port to DIR/port. Control:

    GET  /_mock/state   JSON: acquires, submitted batches, aborts, webhooks
    POST /_mock/fatal   from now on acquire answers 400 with an error body,
                        which makes fishnet log "Stopping queue." and stop
"""

import argparse
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

# A short analysis: the start position and four moves, few nodes, so it
# finishes in seconds on one core of a CI runner.
GAME_ID = "SmokeT01"
BATCH_ID = "smokebatch1"
JOB = {
    "work": {
        "type": "analysis",
        "id": BATCH_ID,
        "nodes": {"classical": 50_000, "sf16": 50_000},
        "timeout": 60_000,
    },
    "game_id": GAME_ID,
    "position": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "variant": "standard",
    "moves": "e2e4 e7e5 g1f3 b8c6",
}

# fishnet only asks for the queue status when it has a backlog setting (it
# can derive one from its own speed). An old "oldest" means: don't wait.
STATUS = {
    "analysis": {
        "user": {"acquired": 0, "queued": 0, "oldest": 3600},
        "system": {"acquired": 0, "queued": 0, "oldest": 3600},
    }
}


class Mock:
    def __init__(self, key):
        self.key = key
        self.lock = threading.Lock()
        self.fatal = False
        self.job_given = False
        self.acquires = 0
        self.submitted = []
        self.aborted = []
        self.webhooks = []

    def state(self):
        with self.lock:
            return {
                "fatal": self.fatal,
                "acquires": self.acquires,
                "submitted": list(self.submitted),
                "aborted": list(self.aborted),
                "webhooks": list(self.webhooks),
            }


def handler(mock):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, fmt, *args):
            sys.stderr.write("mock: %s\n" % (fmt % args))

        def body(self):
            length = int(self.headers.get("Content-Length") or 0)
            return self.rfile.read(length) if length else b""

        def reply(self, status, payload=None):
            data = b"" if payload is None else json.dumps(payload).encode()
            self.send_response(status)
            if payload is not None:
                self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def authorized(self):
            return self.headers.get("Authorization") == f"Bearer {mock.key}"

        def do_GET(self):
            path = self.path.split("?")[0]
            if path == "/_mock/state":
                self.reply(200, mock.state())
            elif path == "/fishnet/key":
                self.reply(200 if self.authorized() else 401)
            elif path == "/fishnet/status":
                self.reply(200, STATUS)
            else:
                self.reply(404)

        def do_POST(self):
            path = self.path.split("?")[0]
            body = self.body()
            if path == "/_mock/fatal":
                with mock.lock:
                    mock.fatal = True
                self.reply(204)
            elif path == "/hook":
                try:
                    event = json.loads(body)
                except ValueError:
                    event = {"unparsable": body.decode(errors="replace")}
                with mock.lock:
                    mock.webhooks.append(event)
                self.reply(200)
            elif path == "/fishnet/acquire":
                with mock.lock:
                    mock.acquires += 1
                    if not self.authorized():
                        status, payload = 401, {"error": "Invalid fishnet key"}
                    elif mock.fatal:
                        status, payload = 400, {"error": "Smoke test: client rejected"}
                    elif not mock.job_given:
                        mock.job_given = True
                        status, payload = 202, JOB
                    else:
                        status, payload = 204, None
                self.reply(status, payload)
            elif path.startswith("/fishnet/analysis/"):
                with mock.lock:
                    mock.submitted.append(path.rsplit("/", 1)[1])
                self.reply(204)
            elif path.startswith("/fishnet/abort/"):
                with mock.lock:
                    mock.aborted.append(path.rsplit("/", 1)[1])
                self.reply(204)
            elif path.startswith("/fishnet/move/"):
                self.reply(204)
            else:
                self.reply(404)

    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--key", required=True)
    parser.add_argument("--dir", required=True, type=Path)
    args = parser.parse_args()

    server = ThreadingHTTPServer(("0.0.0.0", 0), handler(Mock(args.key)))
    args.dir.mkdir(parents=True, exist_ok=True)
    tmp = args.dir / "port.tmp"
    tmp.write_text(str(server.server_address[1]))
    tmp.rename(args.dir / "port")
    server.serve_forever()


if __name__ == "__main__":
    main()
