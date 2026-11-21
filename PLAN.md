# Plan: fishnet image with a monitoring layer

## Goal

A container image that runs the official lichess fishnet client unchanged and adds a
monitoring layer that exposes what the client is doing: throughput, queue state, idle
periods, warnings, errors and fatal stops. The main interface is **our own web
dashboard**, built into the image, so seeing what the client does needs nothing besides
the container. Prometheus metrics are a secondary output for people who already run
Prometheus. It should be usable as a drop-in replacement for `niklasf/fishnet:2`.

Non-goals: changing fishnet's behaviour, forking its source, or talking to the lichess
API ourselves.

## What we have to work with

Findings from the fishnet source (`lichess-org/fishnet`, `src/`):

- **No metrics API.** The only outputs are the log on stdout and a stats file.
- **Stats file** (`stats.rs`): JSON `{total_batches, total_positions, total_nodes}`,
  rewritten after every finished batch. The upstream image disables it unless
  `STATS_FILE` is set.
- **Log lines** (`logger.rs`). Prefixes: none = info, `><> ` = fishnet status, `W: `,
  `E: `, `D: ` (debug, only with `-v`). The lines we care about:
  - `[bar] <url|batch id> finished (<variant>, hce, N knps/core)`: one per analysis batch
  - `><> vX.Y.Z: <nps> (nnue), N batches, N positions, N total nodes`: every 120 s
  - `Going idle for <duration>.`
  - `E: Client update or reconfiguration might be required. Stopping queue.`: fatal
  - Progress line `[bar] N cores, N queued, latest: <url>`: when stdout is not a TTY,
    it's only printed with `-v`
- **Signals** (`main.rs`): the first SIGINT finishes the current batches and then exits,
  a second SIGINT aborts them. SIGTERM stops immediately.
- **Upstream image** (`Dockerfile`, `scripts/docker-entrypoint.sh`): Alpine, user
  `10001`, configured only through env vars (`KEY`, `KEY_FILE`, `CORES`,
  `USER_BACKLOG`, `SYSTEM_BACKLOG`, `MAX_BACKOFF`, `CPU_PRIORITY`, `ENDPOINT`,
  `STATS_FILE`). Always runs with `--no-conf`.

**Risk:** the log format is not a stable interface. Every parser must tolerate unknown
lines, and parsing has to be tested against each new fishnet release (see Phase 4).

## Architecture

```
container (uid 10001)
└─ fishnet-monitor           PID 1, small static binary
   ├─ spawns /docker-entrypoint.sh  (upstream, unchanged) → fishnet
   │    stdout/stderr ──► line parser ──► metrics state
   │                  └─► forwarded to container stdout (unchanged, D: lines optional)
   ├─ watches $STATS_FILE (JSON) ──► metrics state
   ├─ history store (ring buffers, snapshotted to /data/history.json)
   └─ HTTP :9465
        /              dashboard (static HTML/JS/CSS embedded in the binary)
        /api/status    JSON snapshot of the current state
        /api/history   JSON time series for the charts
        /api/events    Server-Sent Events: live batches, state changes, W:/E: lines
        /metrics       Prometheus text format (secondary)
        /healthz       200 / 503
```

- **Wrapper, not sidecar.** One process tree means the monitor sees fishnet's real
  stdout and can tell when fishnet exits. A sidecar would have to scrape
  `docker logs`.
- **The upstream entrypoint stays.** We add a wrapper around it, so every upstream env
  var keeps working.
- **Signals:** forward SIGTERM and SIGINT to the child. Pass on fishnet's exit code
  when it exits. Act as a minimal init and reap zombies (or use `tini`).
- **Changed defaults compared to upstream:**
  - `STATS_FILE=/data/fishnet-stats` and `VOLUME /data`, so the totals survive restarts.
  - Optional `FISHNET_VERBOSE=1` to get the progress line (queue depth). The monitor
    drops the `D:` noise from the forwarded log unless `FORWARD_DEBUG=1` is set.

### Metrics (first set)

| Metric | Type | Source |
|---|---|---|
| `fishnet_up` | gauge | child process alive |
| `fishnet_info{version}` | gauge | summary line |
| `fishnet_batches_total`, `fishnet_positions_total`, `fishnet_nodes_total` | counter | stats file, summary line as fallback |
| `fishnet_batches_finished_total{variant,eval}` | counter | batch-finished line |
| `fishnet_knps_per_core` | gauge + histogram | batch-finished line |
| `fishnet_nnue_nps` | gauge | summary line |
| `fishnet_last_batch_timestamp_seconds` | gauge | batch-finished line |
| `fishnet_state{state="running\|idle\|stopping\|stopped"}` | gauge | idle/stop lines |
| `fishnet_queue_pending`, `fishnet_cores` | gauge | progress line (needs verbose) |
| `fishnet_log_lines_total{level}` | counter | prefix of each line |
| `fishnet_engine_errors_total`, `fishnet_batches_aborted_total` | counter | `E:` / `W:` lines |
| `fishnet_fatal` | gauge | "Stopping queue." line |
| `fishnet_unparsed_lines_total` | counter | lines that match no parser, to spot format drift |

### Dashboard

The monitor has to keep history itself, because no Prometheus is assumed.

**History store**
- Per-minute buckets kept for 7 days: batches, positions, nodes, average knps/core,
  idle seconds. About 10k rows, so it fits easily in memory.
- The last 200 finished batches: time, game URL, variant, eval, knps/core.
- The last 200 `W:`/`E:` lines.
- Snapshotted to `/data/history.json` every minute and on shutdown, so charts survive
  a restart. The volume and file format are ours; fishnet never touches them.

**Page** (one screen, works on a phone)
- **Header:** state badge (running / idle / stopping / stopped / fatal), fishnet
  version, cores, uptime, time since the last batch.
- **Stat tiles:** batches, positions and nodes today and all-time, current knps/core,
  NNUE nps.
- **Charts:** positions per minute (1 h / 24 h / 7 d), knps/core over time, a timeline
  of busy vs idle periods, and batches by variant.
- **Recent batches:** a list of the latest batches, each linking to the analysed game
  on lichess.
- **Problems:** the latest warnings and errors, with fatal ones highlighted.
- **Live updates:** the page loads `/api/status` and `/api/history` once, then follows
  `/api/events`. It doesn't poll.

**Frontend stack:** Preact + htm + `@preact/signals` (~15 KB together) with no build
step, plus one small chart library (uPlot, ~50 KB). All of them are vendored as ES
modules and loaded through an import map, so there's no Node, bundler or
`package.json`. Components use htm's tagged templates instead of JSX. Each SSE event
updates a signal, and only the parts of the page that read it re-render. uPlot charts
are wrapped in small components that call `setData` instead of re-rendering. The files
are embedded in the binary (`rust-embed` or `include_str!`), so the image has no extra
files. Dark and light themes follow `prefers-color-scheme`. If the UI ever outgrows
this (TypeScript, several pages), the components move to Vite + JSX unchanged.

**Security:** the dashboard is read-only and has no login. It never shows the fishnet
key, and the key never goes into history, `/api/*` or the forwarded log. The listen
address is configurable (`MONITOR_LISTEN`, default `0.0.0.0:9465`). Anyone exposing it
beyond the LAN should put it behind a reverse proxy with auth; the README will say so.

### Health

`/healthz` returns 503 in any of these cases:
- fishnet is not running
- `fishnet_fatal == 1`
- there was no batch and no idle line for longer than `STALL_TIMEOUT` (default 15 min)

The Docker `HEALTHCHECK` calls `fishnet-monitor healthcheck`, because the base image
has no curl.

## Technology

- **Monitor: Rust**, built as a static musl binary. That's the same toolchain as
  fishnet, the binary is small, and there's no runtime to add to the image.
  Crates: `tokio`, `axum` (or `hyper`), `prometheus-client`, `serde_json`, `regex`.
- **Image:** multi-stage `Dockerfile`. Stage 1 builds the monitor; stage 2 is
  `FROM docker.io/niklasf/fishnet:2@sha256:…` (pinned digest) plus the binary. Renovate
  (`docker:pinDigests` is already configured) opens a PR whenever upstream publishes
  a new image.
- **Dev env:** add Rust to `devenv.nix` (`languages.rust`, musl target). Don't install
  anything outside it.
- **License:** GPL-3.0, already in `LICENSE`. That matches fishnet. The image gets OCI
  labels pointing to both source repos.

## Phases

1. **Parser + fixtures.** Capture real fishnet output (normal and `-v`) into
   `tests/fixtures/`. Write the line parser and unit tests for every line type above,
   including the variant/hce/`? nps` cases and unknown lines.
2. **Supervisor + HTTP.** Spawn the child, forward the output, handle signals and exit
   codes, read the stats file, and serve `/api/status`, `/metrics` and `/healthz`.
3. **History + dashboard.** Ring buffers and the snapshot file, `/api/history`,
   `/api/events` (SSE), then the embedded page: header and tiles first, then charts,
   recent batches and problems. Vendor the frontend libraries with
   `scripts/vendor-frontend.sh`: it reads pinned versions and SHA-256 sums from
   `frontend/vendor.lock`, downloads the ES module builds into `frontend/vendor/`,
   and fails on a checksum mismatch. Renovate can't see these files, so bump the
   versions by hand and rerun the script. Test the UI with a fake-log mode
   (`fishnet-monitor demo`) that replays the fixtures, so you can work on the UI
   without a real fishnet key.
4. **Image.** Dockerfile, healthcheck, defaults, `VOLUME /data`, README usage section
   (`docker run` and a NixOS `virtualisation.oci-containers` example with `KEY_FILE`).
5. **CI.** GitHub Actions:
   - run the tests
   - build the image, multi-arch amd64/arm64 like upstream
   - a smoke test: start the container with a dummy key against a mock `ENDPOINT` and
     check that `/`, `/api/status` and `/metrics` respond and the startup lines get
     parsed
   - publish to GHCR
   - rerun the fixture tests on every Renovate base-image bump
6. **Optional extras** (only if wanted): ntfy push on a fatal stop or a stall,
   CSV export of the history, alert-rule examples for Prometheus users.

## Open questions

- **Registry and image name:** GHCR `ghcr.io/<user>/fishnet-monitor`?
- **History length:** is 7 days at 1-minute resolution enough, or do you want longer
  history with coarser buckets (for example hourly for a year)?
- **One client or several?** The plan covers one dashboard per container. A single
  dashboard showing several machines would need a separate aggregator, which is a
  different project.
- **Verbose mode on by default?** It's needed for queue depth, but it makes fishnet log
  more. I'd leave it off by default.
- **Port:** `9465` is a placeholder; check it doesn't collide with anything on the host.
