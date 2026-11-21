# template

<!-- One sentence: what this is, written for someone who has never heard of it. -->

A template repository

<!-- Hero screenshot / GIF / photo of the project in action. -->

![screenshot](docs/screenshot.png)

**[Try it here](https://example.com)** <!-- Live demo link: not the repo, not localhost. -->

## About

<!--
- What is it and what does it do?
- Why did you make it? What problem does it solve for you?
-->

## Features

<!-- 3–7 specific bullets a reviewer could verify in the demo. -->

-
-
-

## Usage

<!-- How someone uses the project once it's running. Include controls for games. -->

## Running locally

This project uses [devenv](https://devenv.sh) for its development environment.

```bash
git clone https://github.com/first-uninteresting-username/<repo>
cd <repo>
devenv shell
```

<!-- Add the exact commands to build/run, required env vars (with example values), and any system dependencies. -->

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

<!-- Languages, frameworks, libraries, tools. -->

-

## How it works

<!-- A paragraph or two on interesting technical decisions and trade-offs, problems you hit and how you fixed them. -->

## AI usage

<!-- Be honest: what (if anything) AI was used for, and roughly how much. -->

- `AGENTS.md` is AI written

## Credits

<!-- People who helped, libraries you leaned on, asset packs you used. -->

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
