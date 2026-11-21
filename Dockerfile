# syntax=docker/dockerfile:1@sha256:572cebbc2b15e6f84c077742952674889cb63cd5e9b80abdffcbe587b6f19183

# ---------------------------------------------------------------------------
# Stage 1: build fishnet-monitor as a static musl binary.
#
# rust:*-alpine compiles for *-unknown-linux-musl by default, so the result is
# static without picking a target. The stage runs on the target platform
# (no cross-compiling), so a multi-arch build needs QEMU or native runners.
# ---------------------------------------------------------------------------
FROM docker.io/rust:1.100-alpine3.24@sha256:8d0e82abfdf9a38071e07cabc5afba4ddbe077631a80e4e742a760c38beadc33 AS builder

WORKDIR /src

# Build context is filtered by .dockerignore. frontend/ has to be there because
# the dashboard files get embedded into the binary at compile time.
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY frontend ./frontend

# Cache mounts keep the crate registry and target/ between builds. target/ is
# gone after the RUN finishes, so the binary gets copied out in the same step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked \
 && install -D -m 0755 target/release/fishnet-monitor /out/fishnet-monitor

# ---------------------------------------------------------------------------
# Stage 2: the upstream image, unchanged, plus our binary.
#
# Renovate (docker:pinDigests) updates the digest when upstream pushes a new
# :2 image.
# ---------------------------------------------------------------------------
FROM docker.io/niklasf/fishnet:2@sha256:cfc5b999aa5e61ada4684023e44607723a4926c8391c1d341ae544dee6e1c141

LABEL org.opencontainers.image.title="fishnet-monitor" \
      org.opencontainers.image.description="Official lichess fishnet client with a built-in monitoring dashboard and Prometheus metrics" \
      org.opencontainers.image.source="https://github.com/first-uninteresting-username/fishnet-monitor" \
      org.opencontainers.image.licenses="GPL-3.0-or-later" \
      org.opencontainers.image.base.name="docker.io/niklasf/fishnet:2" \
      fishnet.upstream.source="https://github.com/lichess-org/fishnet"

COPY --from=builder /out/fishnet-monitor /usr/local/bin/fishnet-monitor

# /data holds fishnet's stats file and our history snapshot. It must belong to
# the upstream user, otherwise a fresh named volume is root-owned and fishnet
# can't write to it. Upstream switches to 10001 at the end, so switch back.
USER root
RUN install -d -o 10001 -g 10001 -m 0750 /data
USER 10001:10001

# Defaults that differ from upstream (see PLAN.md). Users can override them.
ENV STATS_FILE=/data/fishnet-stats \
    HISTORY_FILE=/data/history.json \
    MONITOR_LISTEN=0.0.0.0:9465 \
    STALL_TIMEOUT=15m

VOLUME /data
EXPOSE 9465

# The base image has no curl/wget, so the binary checks itself.
HEALTHCHECK --interval=30s --timeout=5s --start-period=2m --retries=3 \
    CMD ["fishnet-monitor", "healthcheck"]

# The monitor is PID 1: it forwards signals, reaps zombies and passes on the
# exit code. CMD is the child it supervises, which is the upstream entrypoint,
# so every upstream env var (KEY, KEY_FILE, CORES, ...) keeps working.
# `docker run <image> demo` still reaches the other subcommands.
#
# Stopping: `docker stop` sends SIGTERM, which the monitor forwards. fishnet
# then drops its current batches at once, like the upstream image does; the
# monitor still saves the history before it exits. For a graceful stop that
# finishes the batches first, run with
#   --stop-signal SIGINT --stop-timeout 60
# (a second SIGINT, or the SIGKILL after the timeout, aborts them).
ENTRYPOINT ["fishnet-monitor"]
CMD ["run", "--", "/docker-entrypoint.sh"]
