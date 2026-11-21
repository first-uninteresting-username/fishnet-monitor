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

## Usage

You need a fishnet key from <https://lichess.org/get-fishnet> and Docker (or
Podman) with Compose. Put the key in a `.env` file next to `compose.yaml`:

```bash
echo 'KEY=your-fishnet-key' > .env
docker compose up -d
```

Open the dashboard on <http://localhost:9465>. Prometheus metrics are on
`/metrics`.

Optional settings for `.env`:

| Variable       | Default | Meaning                                          |
| -------------- | ------- | ------------------------------------------------ |
| `CORES`        | `auto`  | Cores fishnet may use                            |
| `MONITOR_PORT` | `9465`  | Host port of the dashboard                       |
| `WEBHOOK_URL`  | unset   | Endpoint for push notifications (ntfy, Slack, …) |

The stats and the chart history live in the `fishnet-data` volume, so they survive
updates. To update to the newest image:

```bash
docker compose pull && docker compose up -d
```

`docker compose down` stops fishnet gracefully: it finishes its current batches
first (up to 60 seconds).

## Running locally

This project uses [devenv](https://devenv.sh) for its development environment.

```bash
git clone https://github.com/first-uninteresting-username/fishnet-monitor
cd fishnet-monitor
devenv shell
```

Inside the shell:

| Command       | What it does                                                         |
| ------------- | -------------------------------------------------------------------- |
| `demo`        | Dashboard with a fake fishnet on <http://localhost:9465>, no key     |
| `demo-ui`     | Frontend only, with mock data, on <http://localhost:8080/demo/>      |
| `watch-css`   | Rebuilds `frontend/styles.css` while you edit classes                |
| `devenv test` | fmt, clippy and the tests                                            |
| `smoke-test`  | Builds the image with Podman and runs `scripts/smoke-test.sh`        |

To try the image built from your checkout, start the `dev` service. It runs the
demo, so it needs no key:

```bash
docker compose up --build dev
```

The dashboard is on <http://localhost:9465> again. Stop the production service
first if it is running, since both use the same port (or set `MONITOR_PORT`).

## Dependency updates

[Renovate](https://docs.renovatebot.com) (the hosted GitHub app) reads
`renovate.json5`. A custom manager bumps the frontend libraries and the Roboto Flex
font in `frontend/vendor.lock`. Hosted Renovate can't run commands, so
`.github/workflows/refresh-vendored-frontend.yml` finishes each of its PRs: it runs
`scripts/vendor-frontend.sh --update` (new checksums and vendored files), rebuilds
`frontend/styles.css`, and pushes a commit to the PR branch. Review that diff before
merging. For a version change of your own, run `vendor-frontend --update` and
`build-css` in the devenv shell. `devenv.lock` is updated by
`.github/workflows/update-devenv-lock.yml`.

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
  (GPL-3.0-or-later). The Docker image is built on the upstream
  [`niklasf/fishnet`](https://hub.docker.com/r/niklasf/fishnet) image, which
  bundles [Stockfish](https://github.com/official-stockfish/Stockfish) and
  [Fairy-Stockfish](https://github.com/fairy-stockfish/Fairy-Stockfish) (both GPL-3.0).
  Game details come from the [Lichess API](https://lichess.org/api).

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
- [shadcn/ui](https://ui.shadcn.com) (MIT): the components in
  `frontend/src/components/ui/` are ports of it, styled after
  [Material 3](https://m3.material.io)
- [Tailwind CSS](https://tailwindcss.com) (MIT), which generates `frontend/styles.css`
- The Rust crates listed in `Cargo.toml`, among them
  [axum](https://github.com/tokio-rs/axum), [tokio](https://tokio.rs),
  [reqwest](https://github.com/seanmonstar/reqwest) and [serde](https://serde.rs)
  (MIT / Apache-2.0)

## License

This project is licensed under the [GNU AGPL v3.0 or later](LICENSE).
