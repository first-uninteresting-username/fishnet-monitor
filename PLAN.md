# Plan: fishnet image with a monitoring layer

## Goal

A container image that runs the official lichess fishnet client unchanged and adds a
monitoring layer that exposes what the client is doing: throughput, queue state, idle
periods, warnings, errors and fatal stops. It should be usable as a drop-in replacement
for `niklasf/fishnet:2`.

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
   └─ HTTP :9465
        /metrics   Prometheus text format
        /status    JSON snapshot (for humans / simple dashboards)
        /healthz   200 / 503
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
   codes, serve `/metrics`, `/status` and `/healthz`, and read the stats file.
3. **Image.** Dockerfile, healthcheck, defaults, `VOLUME /data`, README usage section
   (`docker run` and a NixOS `virtualisation.oci-containers` example with `KEY_FILE`).
4. **CI.** GitHub Actions:
   - run the tests
   - build the image, multi-arch amd64/arm64 like upstream
   - a smoke test: start the container with a dummy key against a mock `ENDPOINT` and
     check that `/metrics` responds and the startup lines get parsed
   - publish to GHCR
   - rerun the fixture tests on every Renovate base-image bump
5. **Optional extras** (only if wanted): Grafana dashboard JSON, alert-rule examples,
   ntfy push on a fatal stop.

## Open questions

- **Registry and image name:** GHCR `ghcr.io/<user>/fishnet-monitor`?
- **Prometheus only, or also a built-in HTML status page?** I'd start with Prometheus
  plus `/status` JSON and leave out the HTML page.
- **Verbose mode on by default?** It's needed for queue depth, but it makes fishnet log
  more. I'd leave it off by default.
- **Port:** `9465` is a placeholder; check it doesn't collide with anything on the host.
