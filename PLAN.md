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
lines, and parsing has to be tested against each new fishnet release (see phase 5's
format-drift check).

## Architecture

```
container (uid 10001)
└─ fishnet-monitor           PID 1, small static binary
   ├─ spawns /docker-entrypoint.sh  (upstream, unchanged) → fishnet
   │    stdout/stderr ──► line parser ──► metrics state
   │                  └─► forwarded to container stdout (unchanged, D: lines optional)
   ├─ watches $STATS_FILE (JSON) ──► metrics state
   ├─ history store (minute / hour / all-time tiers, snapshotted to /data/history.json)
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
  - No `FISHNET_VERBOSE`: upstream's entrypoint drops extra arguments (it ends in
    `exec /fishnet "${args[@]}"`, no `"$@"`) and fishnet has no env var for `-v`. For
    the progress line (queue depth), override the command and call fishnet directly:
    `<image> run -- /fishnet --no-conf --key-file /key --stats-file /data/fishnet-stats -v`.
    The monitor drops the `D:` noise from the forwarded log unless `FORWARD_DEBUG=1`
    is set.

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

**History store** (`src/history.rs`), in three tiers:
- **Minute buckets, 7 days:** batches, positions, nodes, knps/core by engine, NNUE
  Mnps, idle / stopped / quiet seconds, batches by variant, and looked-up, titled and
  supporter games. About 10k rows. The charts up to 7 days, the "today" counts, the
  sparklines and the 24 h averages come from these.
- **Hour buckets, 7 weeks:** the same counts without speed and variants, for the
  7-week chart.
- **All time, never expires:** only small aggregates: counted batches, positions and
  nodes, batches by variant, game-lookup counts (titled, per title, supporters, bots,
  missing) and the top 50 games by rating (see [Game lookup](#game-lookup)).
- Recent detail besides the buckets: the last 200 finished batches (time, game URL,
  variant, eval, knps/core, lookup result), the last 200 `W:`/`E:` lines, and the 20
  best games of each of the last 8 days (for the 24 h / 7 d top lists).
- Positions and nodes per minute come from changes in the stats file (the summary
  line without one). Time is attributed every 5 s to busy, idle, quiet (no batch and
  no idle line for 3 min) or stopped; minutes with no bucket inside the history count
  as stopped.
- Snapshotted to `HISTORY_FILE` (`/data/history.json` in the image) every minute and
  on shutdown, through a temporary file and a rename, so charts survive a restart. An
  unreadable file is moved aside to `history.json.bad`. The volume and file format are
  ours; fishnet never touches them.

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
- **Charts:** positions per minute (1 h / 6 h / 24 h / 7 d / 7 w), full width and interactive:
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
  `/api/events`. It doesn't poll. The stream has four events (`status`, `live`,
  `lookup`, `charts`), each carrying the latest state of its part of the page; all
  four are sent on connect, then each again when it changes.

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
- **Errors:** a failed request puts its ids back, waits a minute and adds a warning to
  the problems list; the status line shows it.
- **Settings:** `GAME_LOOKUP=0` turns it off; the dashboard then hides the top-games
  and player panels and the player columns. `LICHESS_API` (default `https://lichess.org`) sets the host.
  IDs from batches whose URL host doesn't match it are not looked up, so a dev
  `ENDPOINT` doesn't send unrelated IDs to lichess.org.
- **Demo mode** reads games from `src/demo/games.ndjson` and makes no network
  calls.

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
3. **History + dashboard.** *(Done.)* Ring buffers and the snapshot file, `/api/history`,
   `/api/events` (SSE), then the embedded page: header and tiles first, then charts,
   recent batches and problems. Vendor the frontend libraries with
   `scripts/vendor-frontend.sh`: it reads pinned versions and SHA-256 sums from
   `frontend/vendor.lock`, downloads the ES module builds into `frontend/vendor/`,
   and fails on a checksum mismatch. Renovate bumps the versions through a custom
   manager, and a workflow runs the script with `--update` on its PRs. Test the UI with a fake-log mode
   (`fishnet-monitor demo`) that replays the fixtures on top of 7 weeks of generated
   history, so you can work on the UI without a real fishnet key. Then add the game lookup, the top-games panel and the
   titled/supporter counts, tested against a mock export endpoint (batching, 429
   backoff, missing games, games against the computer, bots, titled players on both
   sides, supporters).
4. **Image.** *(Done.)* Two-stage `Dockerfile` (static musl build, then the pinned
   upstream image plus the binary), OCI labels, `/data` owned by 10001 as a `VOLUME`,
   `ENV` defaults, `HEALTHCHECK` through `fishnet-monitor healthcheck`, and the
   monitor as `ENTRYPOINT` with the upstream entrypoint as `CMD`. No README changes.
   - **Build:** the demo's fixtures (`normal.log`, `problems.log`, `games.ndjson`)
     live in `src/demo/`, so the image build only needs `src/` and `frontend/`; the
     fixture and lookup tests read them from there.
   - **`FISHNET_VERBOSE`:** dropped. Upstream's `docker-entrypoint.sh` has no `"$@"`,
     so a `-v` added to its arguments never reaches fishnet, and fishnet has no env
     var for it. Verbose output needs the command override described under
     [Architecture](#architecture).
   - **Stopping:** `docker stop` sends SIGTERM and fishnet drops its batches at once,
     like upstream. A `Dockerfile` comment gives `--stop-signal SIGINT
     --stop-timeout 60` for a graceful stop. `tests/shutdown.rs` checks that the
     history snapshot is still saved on SIGTERM and the exit code is 143 for a child
     killed by it. The real fishnet catches SIGTERM and exits 0, which the smoke
     test in phase 5 checks.
   - **Volume permissions:** at startup the monitor probes the directories of
     `STATS_FILE` and `HISTORY_FILE` and logs one `W:` line (also in the problems
     list) per directory it can't write, naming the path, the uid and the `chown`
     fix, so a bind mount with the wrong owner shows in the log.
   - `hadolint` passes.

   The image isn't tested by hand in this phase; that's phase 5's smoke test. A local
   build, when one is needed, uses the host's podman (`podman build .`), not a
   runtime added to devenv. The port stays 9465. The container image is the only
   supported way to run the monitor, so the port is set by the image; checking that
   9465 is free on the host is left to whoever deploys it.
5. **CI.** *(Done.)* One workflow, `.github/workflows/ci.yml`, next to the two that
   exist. It runs on pushes to `main`, on `v*` tags, on pull requests (Renovate's
   included) and by hand.
   - **Tests:** `devenv test`, through the same Nix, cachix and devenv steps as
     `update-devenv-lock.yml`. It runs fmt, clippy with `-D warnings`, nextest and all
     the git hooks (hadolint, actionlint, `build-css`, …); a following
     `git diff --exit-code` fails the job when `frontend/styles.css` is stale.
   - **Mock fishnet server:** `scripts/mock-fishnet.py` (Python standard library),
     written from `api.rs` and `queue.rs` in upstream v2.15.1. It answers the key
     check, the queue status (with an old `oldest`, so a derived backlog never makes
     fishnet wait), one small analysis job on the first `acquire` and "no work"
     (204) after that, the analysis submit and aborts, and it receives the webhooks.
     After `POST /_mock/fatal`, `acquire` answers 400 with an error body, which
     fishnet logs as "Server rejected request" and "Stopping queue." before it stops.
     `GET /_mock/state` shows what arrived. The mock is test code; the monitor itself
     still never talks to `/fishnet/*`.
   - **Smoke test:** `scripts/smoke-test.sh <image>` (docker, or `CONTAINER=podman`);
     locally `devenv shell smoke-test` builds with the host's podman and runs it. The
     containers reach the mock on the host through `--add-host
     host.docker.internal:host-gateway`. On a failure it dumps the container and mock
     logs. It checks:
     - the binary has no program interpreter (static), and the image is at most the
       binary's size plus 64 KiB bigger than the base image
     - `demo` with no key serves `/`, `/api/status`, `/metrics` and `/healthz`, and
       exits 0 on `docker stop` (it ignored SIGTERM as PID 1 before; fixed)
     - a real run with `KEY_FILE` mounted read-only, `GAME_LOOKUP=0` and one core:
       fishnet analyses the job, `fishnet_batches_total` is 1, the batch is in
       `/api/history`, `/healthz` is 200 and the `HEALTHCHECK` says healthy
     - **format drift:** `fishnet_unparsed_lines_total` is 0 and
       `fishnet-monitor parse` accepts the whole container log
     - the key appears nowhere in `/api/status`, `/api/history`, `/api/events`,
       `/metrics`, the log, the webhooks or the saved history
     - **stall:** with no more work fishnet prints nothing, so after `STALL_TIMEOUT`
       (45 s here) `/healthz` turns 503, the `HEALTHCHECK` unhealthy, and one `stall`
       webhook arrives
     - `docker stop`: fishnet handles SIGTERM itself ("Stopping now.") and exits 0,
       which the monitor passes on
     - a new container on the same volume still counts the batch (stats file) and
       still lists it (history snapshot)
     - **fatal:** the rejected `acquire` makes fishnet stop its queue and exit 0 on
       its own; the container exits, a `fatal` webhook arrives, and the fatal line is
       in the problems list of the saved history
     The health checks run with `--health-interval 3s --health-retries 1
     --health-start-period 1s`; `0s` would mean "inherit the image's 2 minutes", in
     which failures don't count. With podman, the image must be built with
     `--format docker`, because the OCI format drops the `HEALTHCHECK`.
   - **Image build:** a matrix job on native `ubuntu-24.04` and `ubuntu-24.04-arm`
     runners (no QEMU). Each builds with `docker/build-push-action` (`load: true`,
     `type=gha` cache per arch), runs the smoke test on its image, and on `main` or a
     `v*` tag pushes it by digest; the second build is a cache hit. The `RUN
     --mount=type=cache` cargo caches don't survive between runners, which is
     accepted.
   - **Publish:** a final job joins the digests into one multi-arch manifest on
     `ghcr.io/first-uninteresting-username/fishnet-monitor` with `docker buildx
     imagetools create`, using `GITHUB_TOKEN` (`packages: write` only on the image
     jobs). Tags from `docker/metadata-action`: `edge` and `sha-<short>` from `main`;
     `1.2.3`, `1.2`, `1` and `latest` from a `v1.2.3` tag. The images carry
     `org.opencontainers.image.version` and `.revision`; the other labels stay the
     `Dockerfile`'s. No signing.
   - **Renovate base-image bumps** run the whole workflow, so the format-drift check
     sees the new fishnet's real output before the PR merges. When it fails, capture
     the new lines into `tests/fixtures/` (see its README) and fix the parser in the
     same PR.
   - **Automerge and redeploy:** Renovate merges every update itself (majors and a
     weekly `Cargo.lock` maintenance included) once CI is green on the PR's last
     commit; `platformAutomerge` is off, because without branch protection GitHub's
     auto-merge wouldn't wait for CI. `refresh-vendored-frontend.yml` dispatches CI
     on the branch after its push, so the PR's head commit has checks.
     `update-devenv-lock.yml` merges its PR right after its own `devenv test` and
     dispatches CI on `main`. Every CI run on `main` ends with a `deploy` job that
     POSTs to the orchard deploy webhook (`ORCHARD_DEPLOY_URL` variable,
     `ORCHARD_DEPLOY_TOKEN` secret), which restarts the `fishnet` deployment onto
     the new `:edge`.
   - **Demo on GitHub Pages:** `.github/workflows/pages.yml` publishes `demo/` (the
     real frontend with mock data, no backend) to
     <https://first-uninteresting-username.github.io/fishnet-monitor/> on every push
     to `main` that changes `demo/` or `frontend/`. The site keeps the repository
     layout, because `demo/index.html` loads `../frontend/`; the root redirects to
     `demo/`.
   - **Branch protection: not enabled**, by choice: `main` takes direct pushes.
     CI still gates the automated updates, as above.

## Decisions

- **Registry and image name:** `ghcr.io/first-uninteresting-username/fishnet-monitor`,
  `edge` from `main`, `latest` only from release tags.
- **One client:** one dashboard per container, tied to a single fishnet instance.
  Showing several machines would need a separate aggregator, which is out of scope.
- **Verbose mode:** no switch; verbose output needs the command override (see
  [Architecture](#architecture)).
- **Port:** `9465`, set by the container image, which is the only supported way to
  run the monitor.
