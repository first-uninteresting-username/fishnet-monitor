# Fishnet Monitor

Dashboard for Lichess Fishnet, made in material styling language.

![Screenshot of the dashboard](https://cdn.hackclub.com/01a1ffda-6af2-7876-97a8-6b6e77dc5b0f/fishnet-dashboard-dark.png)

**[Demo](https://first-uninteresting-username.github.io/fishnet-monitor/demo/)**

## About

Fishnet is a tool that lets you donate CPU time to Lichess for game analysis.
It is fine, but there's no real way to see progress, other than one simple JSON file.

Fishnet Monitor is a container image that runs the official Fishnet client and wraps it with a monitoring layer.

## Features

- AIO OCI image
- Familiar UI
- Clear metrics
- Prometheus metrics

## Usage

Get a fishnet key from <https://lichess.org/get-fishnet>, put it in `.env` and
start it:

```bash
echo 'KEY=your-fishnet-key' > .env
docker compose up -d
```

The dashboard is at <http://localhost:9465> and Prometheus metrics are at `/metrics`.

Other things you can set in `.env`:

- `CORES` - how many cores fishnet gets (default `auto`)
- `MONITOR_PORT` - dashboard port (default `9465`)
- `WEBHOOK_URL` - where to send notifications, e.g. ntfy or Slack

Stats and chart history are kept in the `fishnet-data` volume. To update:

```bash
docker compose pull && docker compose up -d
```

`docker compose down` lets fishnet finish its current batches before it stops
(60 seconds max).

## Running locally

You need [devenv](https://devenv.sh).

```bash
git clone https://github.com/first-uninteresting-username/fishnet-monitor
cd fishnet-monitor
devenv shell
```

Then:

- `demo` - the dashboard with a fake fishnet, no key needed
- `demo-ui` - just the frontend with mock data, on <http://localhost:8080/demo/>
- `watch-css` - rebuild the CSS while editing
- `devenv test` - fmt, clippy and tests
- `smoke-test` - build the image with Podman and smoke test it

To try the image from your checkout, run `docker compose up --build dev`. It
runs the demo too, so no key. It uses the same port as the normal service, so
stop that one first or change `MONITOR_PORT`.

## Built with

- Preact
- htm
- Tailwind CSS
- Shadcn UI
- Material styling

## How it works

The monitoring proccess runs as PID 1 inside the container and starts upstream `/docker-entrypoint.sh` as a child process and reads its logs.

## AI usage

- `AGENTS.md` is AI written
- The whole project, except for the README is AI made

## Credits

- [fishnet](https://github.com/lichess-org/fishnet) by the lichess.org team
  (GPL-3.0-or-later). The image is based on
  [`niklasf/fishnet`](https://hub.docker.com/r/niklasf/fishnet), which includes
  [Stockfish](https://github.com/official-stockfish/Stockfish) and
  [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) (both GPL-3.0).
- Game info comes from the [Lichess API](https://lichess.org/api).

Vendored into `frontend/vendor/` (versions in `frontend/vendor.lock`):

- [Preact](https://preactjs.com) and [Preact Signals](https://github.com/preactjs/signals) (MIT)
- [htm](https://github.com/developit/htm) (Apache-2.0)
- [uPlot](https://github.com/leeoniya/uPlot) by Leon Sorokin (MIT)
- [clsx](https://github.com/lukeed/clsx) (MIT)
- [tailwind-merge](https://github.com/dcastil/tailwind-merge) (MIT)
- [class-variance-authority](https://github.com/joe-bell/cva) (Apache-2.0)
- [Roboto Flex](https://github.com/googlefonts/roboto-flex) by the Roboto Flex
  Project Authors, via [Fontsource](https://fontsource.org) (SIL Open Font License 1.1;
  license in `frontend/vendor/roboto-flex-LICENSE.txt`)

Also used:

- [Material Symbols](https://github.com/google/material-design-icons) by Google
  (Apache-2.0) for the icons and favicon
- [shadcn/ui](https://ui.shadcn.com) (MIT), components ported to
  `frontend/src/components/ui/` and restyled to look like
  [Material 3](https://m3.material.io)
- [Tailwind CSS](https://tailwindcss.com) (MIT)
- Rust crates from `Cargo.toml`, like
  [axum](https://github.com/tokio-rs/axum), [tokio](https://tokio.rs),
  [reqwest](https://github.com/seanmonstar/reqwest) and [serde](https://serde.rs)
  (MIT / Apache-2.0)

## License

This project is licensed under the [GNU AGPL v3.0 or later](LICENSE).
