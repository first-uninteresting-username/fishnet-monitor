# Log fixtures

fishnet output that `tests/fixtures.rs` runs through the parser.

| File | What it covers |
|---|---|
| `normal.log` | A default container run (no `-v`): banner, configuration, batches, idle, the 2-minute summary, SIGINT shutdown |
| `verbose.log` | A `-v` run: `D:` lines, progress lines with queue depth, a move batch, a short (debug) idle, `???` uncertainty |
| `variants.log` | Every variant short name, `hce`, `? nps`, batches without a game, a dev endpoint |
| `problems.log` | Warnings and errors, engine timeouts and crashes, an aborted batch, the rejection and fatal stop |
| `panic.log` | A Rust panic (missing key file) |
| `unknown.log` | Lines the parser must tolerate: changed formats and new messages |

## Where they come from

The lines are built from the format strings in fishnet's source
(`lichess-org/fishnet` at `3d80022`, version 2.15.2-dev; files `logger.rs`,
`queue.rs`, `main.rs`, `api.rs`, `stockfish.rs`, `configure.rs` and
`stats.rs`), with queue bars computed the way `QueueStatusBar` draws them.
They aren't captures of a live client yet, because that needs a fishnet key.

## Capturing real output

When fishnet changes its output, capture a run and add it here:

```bash
# Normal run (stdout isn't a TTY, like in the container)
docker run --rm -e KEY_FILE=/key -v ./key:/key:ro niklasf/fishnet:2 2>&1 | tee normal-capture.log
# Verbose run
docker run --rm -e KEY_FILE=/key -v ./key:/key:ro --entrypoint /fishnet niklasf/fishnet:2 \
  --no-conf --key-file /key -v 2>&1 | tee verbose-capture.log
```

Check the capture with `cargo run -- parse verbose-capture.log`. It prints the
event for every line, marks unparsed lines with `?`, and exits with 1 if there
are any. Remove anything private (the key never appears in the log, but check
error lines with URLs) before committing.
