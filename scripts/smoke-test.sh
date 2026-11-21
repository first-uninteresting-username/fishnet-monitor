#!/usr/bin/env bash
# Smoke test for the image: runs it the way users do and checks what the
# unit tests can't (PLAN.md, phase 5).
#
#   scripts/smoke-test.sh IMAGE
#
# Needs docker (or CONTAINER=podman), curl and python3. Locally,
# `devenv shell smoke-test` builds the image with podman and runs this. The real fishnet in
# the image works against scripts/mock-fishnet.py on the host, which hands
# out one small analysis job. With podman, build the image with
# `podman build --format docker`: the OCI format drops the HEALTHCHECK.
#
# Every check prints "ok" or "FAIL" with what it expected and got. On a
# failure the script dumps the container and mock logs and exits 1.

set -euo pipefail

IMAGE=${1:?usage: smoke-test.sh IMAGE}
DOCKER=${CONTAINER:-docker}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
# How the containers reach the mock on the host.
MOCK_HOST=${MOCK_HOST:-host.docker.internal}

work=$(mktemp -d)
run_id=smoke-$$-$RANDOM
key="smoke$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
volume=$run_id-data
containers=()
mock_pid=
failed=0

cleanup() {
  local status=$?
  if [ "$failed" -ne 0 ] || [ "$status" -ne 0 ]; then
    for c in "${containers[@]}"; do
      echo "::group::logs of $c"
      "$DOCKER" logs "$c" 2>&1 | tail -n 200 || true
      echo "::endgroup::"
    done
    if [ -f "$work/mock.log" ]; then
      echo "::group::mock log"
      tail -n 200 "$work/mock.log"
      echo "::endgroup::"
    fi
  fi
  for c in "${containers[@]}"; do "$DOCKER" rm -f "$c" >/dev/null 2>&1 || true; done
  "$DOCKER" volume rm -f "$volume" >/dev/null 2>&1 || true
  [ -n "$mock_pid" ] && kill "$mock_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

ok() { echo "ok   $*"; }
fail() {
  echo "FAIL $*"
  failed=1
  exit 1
}

# check DESCRIPTION EXPECTED ACTUAL
check() {
  if [ "$2" = "$3" ]; then ok "$1"; else fail "$1: expected '$2', got '$3'"; fi
}

# wait_for DESCRIPTION SECONDS COMMAND...: retry COMMAND once a second.
wait_for() {
  local what=$1 timeout=$2
  shift 2
  for _ in $(seq "$timeout"); do
    if "$@" >/dev/null 2>&1; then
      ok "$what"
      return
    fi
    sleep 1
  done
  fail "$what: not within ${timeout}s"
}

# until_eq DESCRIPTION SECONDS EXPECTED COMMAND...: rerun COMMAND once a
# second until it prints EXPECTED.
until_eq() {
  local what=$1 timeout=$2 want=$3 got=
  shift 3
  for _ in $(seq "$timeout"); do
    got=$("$@" 2>/dev/null || true)
    if [ "$got" = "$want" ]; then
      ok "$what"
      return
    fi
    sleep 1
  done
  fail "$what: expected '$want' within ${timeout}s, last got '$got'"
}

# json URL EXPRESSION: evaluate a Python expression on the JSON at URL (as d).
json() {
  curl -fsS --max-time 5 "$1" | python3 -c \
    'import json, sys; d = json.load(sys.stdin); print(json.dumps(eval(sys.argv[1])))' "$2"
}

# metric URL NAME: the value of an unlabelled metric.
metric() {
  curl -fsS --max-time 5 "$1/metrics" | awk -v m="$2" '$1 == m { print $2 }'
}

# start NAME ARGS...: run the image detached with the dashboard on a free port.
start() {
  local name=$run_id-$1
  shift
  "$DOCKER" run -d --name "$name" -p 127.0.0.1::9465 "$@" >/dev/null
  containers+=("$name")
  url=http://$("$DOCKER" port "$name" 9465/tcp | head -n 1)
  cname=$name
}

health() { "$DOCKER" inspect -f '{{.State.Health.Status}}' "$1"; }
http_code() { curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$1"; }
mock_state() { json "http://127.0.0.1:$mock_port/_mock/state" "$1"; }

# --- Static and small --------------------------------------------------------

echo "== binary and size"
probe=$run_id-probe
"$DOCKER" create --name "$probe" "$IMAGE" >/dev/null
containers+=("$probe")
"$DOCKER" cp "$probe:/usr/local/bin/fishnet-monitor" "$work/fishnet-monitor"
"$DOCKER" rm "$probe" >/dev/null
# A static (or static-pie) binary has no program interpreter (PT_INTERP).
interp=$(python3 - "$work/fishnet-monitor" <<'PY'
import struct, sys
data = open(sys.argv[1], "rb").read()
assert data[:4] == b"\x7fELF" and data[4] == 2, "not a 64-bit ELF"
phoff, = struct.unpack_from("<Q", data, 0x20)
phentsize, phnum = struct.unpack_from("<HH", data, 0x36)
types = [struct.unpack_from("<I", data, phoff + i * phentsize)[0] for i in range(phnum)]
print("yes" if 3 in types else "no")
PY
)
check "binary has no dynamic loader (static)" no "$interp"

base=$(awk '/^FROM / { ref = $2 } END { print ref }' "$ROOT/Dockerfile")
"$DOCKER" pull -q "$base" >/dev/null
bin_size=$(stat -c %s "$work/fishnet-monitor")
image_size=$("$DOCKER" image inspect -f '{{.Size}}' "$IMAGE")
base_size=$("$DOCKER" image inspect -f '{{.Size}}' "$base")
growth=$((image_size - base_size))
# 64 KiB for /data and the layer metadata.
if [ "$growth" -le $((bin_size + 65536)) ]; then
  ok "image grows by $growth bytes, binary is $bin_size"
else
  fail "image grows by $growth bytes, more than the $bin_size byte binary + 64 KiB"
fi

# --- Demo --------------------------------------------------------------------

echo "== demo"
start demo "$IMAGE" demo
wait_for "demo serves /healthz" 30 curl -fsS --max-time 2 "$url/healthz"
curl -fsS "$url/" | grep -qi '<!doctype html' || fail "demo: / is not the page"
ok "demo serves the page"
check "demo /api/status has the demo host" '"demo"' "$(json "$url/api/status" 'd["host"]')"
check "demo /metrics has fishnet_up" 1 "$(metric "$url" fishnet_up)"
# The demo is PID 1 too: it must stop on SIGTERM, not wait for the SIGKILL.
"$DOCKER" stop -t 10 "$cname" >/dev/null
check "demo exits cleanly on docker stop" 0 "$("$DOCKER" inspect -f '{{.State.ExitCode}}' "$cname")"
"$DOCKER" rm "$cname" >/dev/null

# --- Real run against the mock -------------------------------------------------

echo "== real run"
python3 "$ROOT/scripts/mock-fishnet.py" --key "$key" --dir "$work/mock" >"$work/mock.log" 2>&1 &
mock_pid=$!
wait_for "mock is up" 10 test -f "$work/mock/port"
mock_port=$(cat "$work/mock/port")
mock_url=http://$MOCK_HOST:$mock_port

printf '%s\n' "$key" >"$work/key"
chmod 0644 "$work/key"
"$DOCKER" volume create "$volume" >/dev/null

run_args=(
  --add-host "$MOCK_HOST:host-gateway"
  -v "$volume:/data"
  -v "$work/key:/run/fishnet-key:ro"
  -e KEY_FILE=/run/fishnet-key
  -e "ENDPOINT=$mock_url/fishnet"
  -e CORES=1
  -e GAME_LOOKUP=0
  -e STALL_TIMEOUT=45s
  -e "WEBHOOK_URL=$mock_url/hook"
  -e MONITOR_HOST_NAME=smoke
  --health-interval 3s
  --health-retries 1
  --health-start-period 1s  # 0s would mean "inherit the image's 2m"
  "$IMAGE"
)
start run1 "${run_args[@]}"
run1=$cname

until_eq "fishnet takes the job and sends the analysis back" 120 1 \
  mock_state 'len(d["submitted"])'
until_eq "the stats file counts the batch" 20 1 metric "$url" fishnet_batches_total
check "fishnet runs" 1 "$(metric "$url" fishnet_up)"
check "the batch is in the recent batches" '"SmokeT01"' \
  "$(json "$url/api/history" 'd["batches"][0]["id"]')"
check "state" '"running"' "$(json "$url/api/status" 'd["state"]')"
check "/healthz" 200 "$(http_code "$url/healthz")"
until_eq "HEALTHCHECK reports healthy" 30 healthy health "$run1"

echo "== format drift"
check "no unparsed fishnet lines" 0 "$(metric "$url" fishnet_unparsed_lines_total)"
"$DOCKER" logs "$run1" >"$work/run1.log" 2>&1
if "$DOCKER" run --rm -i --entrypoint fishnet-monitor "$IMAGE" parse <"$work/run1.log" >"$work/parse.out"; then
  ok "fishnet-monitor parse accepts the whole log"
else
  grep '^?' "$work/parse.out" || true
  fail "fishnet-monitor parse found lines it doesn't know (above)"
fi

echo "== no key leak"
curl -fsS "$url/api/status" >"$work/leak"
curl -fsS "$url/api/history" >>"$work/leak"
# The stream never ends; curl gives up after 3 s with what it got.
curl -s -N --max-time 3 "$url/api/events" >>"$work/leak" || true
curl -fsS "$url/metrics" >>"$work/leak"
"$DOCKER" logs "$run1" >>"$work/leak" 2>&1
grep -q 'event: status' "$work/leak" || fail "/api/events sent no status event"
if grep -qF "$key" "$work/leak"; then fail "the key shows up in the API, metrics or log"; fi
ok "the key appears nowhere in /api/*, /metrics or the log"

echo "== stall"
# The mock has no more work, so fishnet prints nothing until STALL_TIMEOUT.
until_eq "/healthz turns 503 on a stall" 90 503 http_code "$url/healthz"
check "status says stalled" true "$(json "$url/api/status" 'd["stalled"]')"
until_eq "HEALTHCHECK reports unhealthy" 30 unhealthy health "$run1"
until_eq "the stall webhook arrives" 30 '["stall"]' \
  mock_state '[w["event"] for w in d["webhooks"]]'
check "webhook host" '"smoke"' "$(mock_state 'd["webhooks"][0]["host"]')"
if mock_state 'd["webhooks"]' | grep -qF "$key"; then fail "the key is in a webhook"; fi
ok "no key in the webhook"

echo "== stop"
"$DOCKER" stop -t 30 "$run1" >/dev/null
# fishnet handles SIGTERM itself and exits 0; the monitor passes that on.
check "exit code after docker stop" 0 "$("$DOCKER" inspect -f '{{.State.ExitCode}}' "$run1")"
grep -q 'Stopping now' <("$DOCKER" logs "$run1" 2>&1) || fail "fishnet didn't log 'Stopping now.'"
ok "fishnet stopped on SIGTERM"
"$DOCKER" rm "$run1" >/dev/null

# --- Restart on the same volume -------------------------------------------------

echo "== restart"
start run2 "${run_args[@]}"
run2=$cname
wait_for "the new container serves /healthz" 30 curl -fsS --max-time 2 "$url/healthz"
until_eq "the stats file still counts the batch" 20 1 metric "$url" fishnet_batches_total
check "the history snapshot still has the batch" '"SmokeT01"' \
  "$(json "$url/api/history" 'd["batches"][0]["id"]')"

# --- Fatal -------------------------------------------------------------------

echo "== fatal"
curl -fsS -X POST "http://127.0.0.1:$mock_port/_mock/fatal"
# fishnet asks again within its no-work backoff, logs the rejection, stops
# its queue and exits; the monitor exits with it.
until_eq "fishnet exits after the rejection" 120 false \
  "$DOCKER" inspect -f '{{.State.Running}}' "$run2"
"$DOCKER" logs "$run2" >"$work/run2.log" 2>&1
grep -q 'Stopping queue' "$work/run2.log" || fail "no 'Stopping queue.' in the log"
ok "fishnet logged the fatal stop"
check "exit code after the fatal stop" 0 "$("$DOCKER" inspect -f '{{.State.ExitCode}}' "$run2")"
until_eq "the fatal webhook arrives" 30 '["fatal"]' \
  mock_state '[w["event"] for w in d["webhooks"] if w["event"] == "fatal"][:1]'
# The problems list is saved with the history on exit.
"$DOCKER" run --rm -v "$volume:/data" --entrypoint cat "$IMAGE" /data/history.json >"$work/history.json"
grep -q 'Stopping queue' "$work/history.json" || fail "the fatal line isn't in the problems list"
ok "the fatal line is in the problems list"
if grep -qF "$key" "$work/run2.log" "$work/history.json"; then fail "the key is in the log or history"; fi
ok "no key in the log or the saved history"

echo "all smoke checks passed"
