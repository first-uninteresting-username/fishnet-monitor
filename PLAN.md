# Plan: fishnet image with a monitoring layer

## Goal

A container image that runs the official lichess fishnet client unchanged and adds a
monitoring layer that exposes what the client is doing: throughput, queue state, idle
periods, warnings, errors and fatal stops. The main interface is **our own web
dashboard**, built into the image, so seeing what the client does needs nothing besides
the container. Prometheus metrics are a secondary output for people who already run
Prometheus. It should be usable as a drop-in replacement for `niklasf/fishnet:2`.

Non-goals: changing fishnet's behaviour, forking its source, or talking to the fishnet
work API (`/fishnet/*`) ourselves. The only lichess call we make is a read-only lookup of
public game data for the top-games list and the player counts (see
[Game lookup](#game-lookup)), and it can be turned off.

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
   ├─ game lookup: batched game IDs ──► lichess.org/api/games/export/_ids ──► ratings
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
| `fishnet_top_game_rating` | gauge | highest game rating seen so far (game lookup) |
| `fishnet_game_lookups_total{result="ok\|missing\|error\|ratelimited"}` | counter | game lookup |
| `fishnet_games_titled_total`, `fishnet_games_titled_by_title_total{title}` | counter | game lookup |
| `fishnet_games_supporter_total` | counter | game lookup |

### Dashboard

The monitor has to keep history itself, because no Prometheus is assumed.

**History store**
- Per-minute buckets kept for 7 days: batches, positions, nodes, average knps/core,
  idle seconds. About 10k rows, so it fits easily in memory.
- The last 200 finished batches: time, game URL, variant, eval, knps/core.
- The last 200 `W:`/`E:` lines.
- The top 50 games by rating, all-time (see [Game lookup](#game-lookup)). This list is
  not cut off after 7 days.
- The per-minute buckets also count looked-up, titled and supporter games. All-time
  totals for these (including one per title) are kept separately and never expire.
- Snapshotted to `/data/history.json` every minute and on shutdown, so charts survive
  a restart. The volume and file format are ours; fishnet never touches them.

**Page** (one screen, works on a phone)
- **Header:** state badge (running / idle / stopping / stopped / fatal), health,
  fishnet version, cores, uptime, time since the last batch.
- **Stall warning:** after 3 minutes with no batch and no idle line, a strip under the
  header counts down to `STALL_TIMEOUT`, so a stuck client shows before `/healthz`
  turns unhealthy.
- **Stale data:** when fishnet isn't running or the page loses `/api/events`, the
  tiles and panels are dimmed and a strip says how old the data is (and that the page
  is reconnecting).
- **Stat tiles:** batches, positions and nodes today and all-time, current knps/core
  with the 24 h average split by engine (nnue / hce), NNUE nps.
- **Charts:** positions per minute (1 h / 6 h / 24 h / 7 d), full width and interactive:
  a hover/keyboard tooltip with the value and how it compares to the average, drag to
  zoom, double-click or Esc to reset, idle stretches shaded, and avg / peak / idle time
  for the visible window. Idle stretches and stopped periods are shaded in this chart,
  so there's no separate busy/idle timeline. Batches by variant use a linear scale with
  each variant's share. No separate knps/core chart; speed stays in the stat tile.
- **Recent batches:** a list of the latest batches, each linking to the analysed game
  on lichess, with the players' ratings once the lookup has them.
- **Top games:** the highest-rated games this client has analysed, with a 24 h / 7 d /
  all-time switch. Each row shows the rank, white and black with their ratings, the
  game rating, the speed and variant, the date, and a link to the game. A status line
  shows whether the game lookup is working or rate-limited (with the wait), how many
  games are queued, and how many lichess couldn't find.
- **Titled players and supporters:** how many games had a titled player or a lichess
  supporter, today and all-time, as a count and as a share of looked-up games, plus a
  breakdown by title.
- **Problems:** the latest warnings and errors, with fatal ones highlighted. Lines that
  differ only in numbers are grouped with a count and first/last seen, and an
  errors / warnings filter.
- **Phones:** tables turn into stacked cards below ~600 px instead of scrolling sideways.
- **Empty states:** on first start every panel says what will appear and when (for
  example the version waits for the first 2-minute summary). With `GAME_LOOKUP=0` the
  player panels and columns are hidden.
- **Live updates:** the page loads `/api/status` and `/api/history` once, then follows
  `/api/events`. It doesn't poll.

**Frontend stack:** Preact + htm + `@preact/signals` (~15 KB together) with no build
step, plus one small chart library (uPlot, ~50 KB). All of them are vendored as ES
modules and loaded through an import map, so there's no Node, bundler or
`package.json`. Styling is Tailwind CSS v4 only, with no hand-written CSS: components
use utility classes, and `frontend/tailwind.css` holds just the theme tokens and a
few base rules. The standalone Tailwind CLI (from nixpkgs, in devenv) compiles it to
`frontend/styles.css` with `build-css` (or `watch-css`); a pre-commit hook rebuilds it,
and the output is committed so the Rust and Docker builds don't need Tailwind. UI
elements are [shadcn/ui](https://ui.shadcn.com) components (Card, Button, Badge,
Alert, Progress, Table, ToggleGroup, Empty), ported to htm in
`frontend/src/components/ui/` and restyled to [Material 3](https://m3.material.io):
elevated cards for every panel and tile, segmented buttons (with a check on the
selected segment) for the range and filter switches, full-round buttons with M3 state
layers, banners for the stall / fatal / stale strips, a linear progress indicator,
and M3's shape (4 / 8 / 12 / 16 / 28 px), elevation, type and easing tokens. Their
helpers `clsx`, `tailwind-merge` and `class-variance-authority` are vendored like the
rest. Icons and the logo are [Material Symbols](https://fonts.google.com/icons)
(Rounded) inlined as SVG in `frontend/src/components/icons.js`; the logo is
`phishing` (a fish hook) on primary-container, and `frontend/favicon.svg` is the same
mark. Text is set in Roboto Flex, vendored like the libraries (from
`@fontsource-variable/roboto-flex`: the weight axis only, as two `woff2` files for
Latin and Latin Extended, ~57 KB together, SIL OFL 1.1) and served with the page, so
it looks the same everywhere and makes no outside request. Mono text uses Roboto
Mono when installed. The colour scheme is mapped to shadcn's token names
(`--background` = surface, `--card` = surface-container-low, `--muted-foreground` =
on-surface-variant, `--secondary` = secondary-container, …) plus `--ok`, `--warn`,
`--crit`, `--idle`, `--hce` and `--title`. Components use htm's tagged templates instead of JSX. Each SSE event
updates a signal, and only the parts of the page that read it re-render. uPlot charts
are wrapped in small components that call `setData` instead of re-rendering. The files
are embedded in the binary (`rust-embed` or `include_str!`), so the image has no extra
files. Dark and light themes follow `prefers-color-scheme`. The scheme is an M3
tonal-spot scheme built from the palette
[`#14110f` `#34312d` `#7e7f83` `#d9c5b2`](https://coolors.co/14110f-34312d-7e7f83-d9c5b2):
primary and secondary take the hue of `#7e7f83` (a slate blue), neutrals and tertiary
take the hue of `#d9c5b2`, so surfaces stay warm, and roles use M3's tones (light:
primary T40 on surface T98; dark: primary T80 on surface T6). Status colours (ok,
warn, crit, idle) and the hce and title colours are custom colours at T40 / T80;
all of them meet WCAG AA as text on every surface. If the UI ever outgrows
this (TypeScript, several pages), the components move to Vite + JSX, and `ui/` can be
replaced by the real shadcn/ui components.

**Security:** the dashboard is read-only and has no login. It never shows the fishnet
key, and the key never goes into history, `/api/*` or the forwarded log. The listen
address is configurable (`MONITOR_LISTEN`, default `0.0.0.0:9465`). Anyone exposing it
beyond the LAN should put it behind a reverse proxy with auth; the README will say so.

### Game lookup

The top-games list and the titled/supporter counts need player data, and fishnet
itself can't provide it. The work it gets from lichess has a `game_id` but no players,
ratings or titles (`AcquireResponseBody` in `api.rs`). So the monitor looks the games up
on the public lichess API.

- **Game IDs:** taken from the batch-finished line. Its URL is `<endpoint>/<game_id>`.
  Batches without a game (the line says `batch <id> finished`) can't be ranked and are
  skipped.
- **Lookup:** queue the IDs and send them every 60 s, or as soon as 300 are waiting, to
  `POST https://lichess.org/api/games/export/_ids` (up to 300 IDs per request,
  `Accept: application/x-ndjson`, `moves=false`, no auth). Only one request runs at a
  time. On a 429 the monitor waits a full minute, as the lichess API docs ask. If a
  request fails, the IDs go back into the queue and the queue is capped, so an outage
  can't make it grow without limit. Games the API doesn't return (deleted, or a
  different endpoint) are counted as `missing`.
- **Game rating:** the average of both players' ratings at the time of the game
  (decided; not the stronger player's rating). We also store both ratings, the speed, the
  variant, whether the game was rated, and its date. Games against the computer or with
  a side that has no rating are left out of the ranking.
- **Titled games:** a game counts once if at least one player has a `title` (GM, IM,
  FM, CM, NM, WGM, WIM, WFM, WCM, WNM, or LM, the Lichess Master title). `BOT` is also
  a `title` value in the API, but bot accounts aren't titled players, so their games
  are counted separately. Each title also gets its own counter. A GM-vs-IM game counts
  for both, so the per-title counts can add up to more than the total.
- **Supporter games:** a game counts once if at least one player has `patronColor` set,
  which the API docs give as the sign of an active Patron. The older boolean `patron`
  field is deprecated.
- **Caveat:** the export shows a player's title and supporter status as of the lookup,
  not as of the game. Because the lookup runs within a minute or two of the analysis,
  the difference doesn't matter in practice.
- **Settings:** `GAME_LOOKUP=0` turns it off; the dashboard then hides the top-games
  and player panels and the player columns. `LICHESS_API` (default `https://lichess.org`) sets the host.
  IDs from batches whose URL host doesn't match it are not looked up, so a dev
  `ENDPOINT` doesn't send unrelated IDs to lichess.org.
- **Demo mode** reads ratings from a fixture file and makes no network calls.

### Health

`/healthz` returns 503 in any of these cases:
- fishnet is not running
- `fishnet_fatal == 1`
- there was no batch and no idle line for longer than `STALL_TIMEOUT` (default 15 min)

The Docker `HEALTHCHECK` calls `fishnet-monitor healthcheck`, because the base image
has no curl.

### Webhooks

The monitor can push a JSON notification to any HTTP endpoint (ntfy, Slack-compatible
receivers, a home-automation hook, and so on) when fishnet needs attention.

- **Setting:** `WEBHOOK_URL`. Unset means no webhook calls at all.
- **Events:**
  - `fatal`: the "Stopping queue." line, or fishnet exits with an error.
  - `stall`: the stall timeout (`STALL_TIMEOUT`) has passed with no batch and no idle line.
    It fires once per stall and re-arms after the next batch or idle line.
- **Payload:** `POST` with `Content-Type: application/json`:
  `{"event": "fatal" | "stall", "time": <RFC 3339>, "host": <MONITOR_HOST_NAME or hostname>,
  "state": <current state>, "message": <the triggering log line or a short description>}`.
  The fishnet key never goes into the payload.
- **Delivery:** the request runs on its own task with a 10 s timeout. It retries 3 times
  with backoff (1 s, 5 s, 30 s). A final failure logs a `W:`-style line in the monitor's
  own problems list and doesn't block or crash anything. Delivery is best-effort; the
  dashboard and `/healthz` stay the source of truth.
- **Scope:** only these two events for now. More events, or a different body format per
  receiver, can be added later if needed.
- **Tests:** against a mock HTTP receiver (fire once per stall, re-arm, retries, timeout,
  no key in the payload, unset `WEBHOOK_URL` sends nothing).

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
   codes, read the stats file, serve `/api/status`, `/metrics` and `/healthz`, and send
   the [webhooks](#webhooks) for `fatal` and `stall`.
3. **History + dashboard.** Ring buffers and the snapshot file, `/api/history`,
   `/api/events` (SSE), then the embedded page: header and tiles first, then charts,
   recent batches and problems. Vendor the frontend libraries with
   `scripts/vendor-frontend.sh`: it reads pinned versions and SHA-256 sums from
   `frontend/vendor.lock`, downloads the ES module builds into `frontend/vendor/`,
   and fails on a checksum mismatch. Renovate bumps the versions through a custom
   manager, and a workflow runs the script with `--update` on its PRs. Test the UI with a fake-log mode
   (`fishnet-monitor demo`) that replays the fixtures, so you can work on the UI
   without a real fishnet key. Then add the game lookup, the top-games panel and the
   titled/supporter counts, tested against a mock export endpoint (batching, 429
   backoff, missing games, games against the computer, bots, titled players on both
   sides, supporters).
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
