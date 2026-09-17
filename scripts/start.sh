#!/bin/sh
# Start Knowlith: daemon + curl setup (same as the UI) + browser fullscreen.
#
#   sh scripts/start.sh
#   sh scripts/start.sh --fresh
#   KNOWLITH_ENGINE=cursor-agent sh scripts/start.sh --fresh --demo
#
# The engine in curl policy and `serve --engine` must match — the worker binds
# at start; curl only persists what the UI would show.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KNOWLITH_HOME="${KNOWLITH_HOME:-$HOME/Knowlith}"
PORT="${KNOWLITH_PORT:-7717}"
BASE="http://127.0.0.1:${PORT}"
TOKEN_FILE="$KNOWLITH_HOME/api.token"
ENGINE="${KNOWLITH_ENGINE:-cursor-agent}"
COMPANY_NAME="${KNOWLITH_COMPANY:-Demo}"
COMPANY_PROFILE="${KNOWLITH_PROFILE:-We send invoices to Formify. Bethel AI issues them in EUR with net-7 payment terms.}"
DEMO="${KNOWLITH_DEMO:-$HOME/Documents/knowlith-demo/invoices}"

FRESH=0
WITH_DEMO=0
for arg in "$@"; do
  case "$arg" in
    --fresh) FRESH=1 ;;
    --demo) WITH_DEMO=1 ;;
    *) printf 'unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

say() { printf '  %s\n' "$*"; }
step() { printf '\n\033[1m%s\033[0m\n' "$*"; }

api() {
  curl -sf -H "x-knowlith-token: $(cat "$TOKEN_FILE")" "$@"
}

wait_for_daemon() {
  for _ in $(seq 1 60); do
    if [ -f "$TOKEN_FILE" ] && api "$BASE/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  say "daemon never became healthy — see $KNOWLITH_HOME/dev.log"
  tail -20 "$KNOWLITH_HOME/dev.log" 2>/dev/null || true
  return 1
}

open_fullscreen() {
  url=$1
  if [ -d "/Applications/Google Chrome.app" ]; then
    open -na "Google Chrome" --args --start-fullscreen "$url" 2>/dev/null && return 0
  fi
  if [ -d "/Applications/Brave Browser.app" ]; then
    open -na "Brave Browser" --args --start-fullscreen "$url" 2>/dev/null && return 0
  fi
  open "$url"
  sleep 1.5
  osascript -e 'tell application "System Events" to keystroke "f" using {command down, control down}' 2>/dev/null || true
}

stop_all() {
  trap - INT TERM EXIT
  [ -n "${DAEMON:-}" ] && kill "$DAEMON" 2>/dev/null || true
  wait 2>/dev/null || true
  printf '\nstopped\n\n'
}

step 'Knowlith — start'
say "engine: $ENGINE"
say "home:   $KNOWLITH_HOME"
say "url:    $BASE"

step 'Stopping anything on 7717 / 5173'
pkill -f 'knowlith serve' 2>/dev/null || true
pkill -f 'vite.*knowlith' 2>/dev/null || true
sleep 1

if [ "$FRESH" = "1" ] && [ -d "$KNOWLITH_HOME" ]; then
  step 'Fresh company'
  ASIDE="$KNOWLITH_HOME.$(date +%Y%m%d-%H%M%S)"
  mv "$KNOWLITH_HOME" "$ASIDE"
  say "previous company moved to $ASIDE"
fi
mkdir -p "$KNOWLITH_HOME"

step 'Building'
cargo build -q --manifest-path "$ROOT/Cargo.toml" -p knowlith-cli

KNOWLITH="$ROOT/target/debug/knowlith"
export KNOWLITH_REPO="$ROOT"

step "Starting daemon (engine=$ENGINE)"
"$KNOWLITH" serve --port "$PORT" --engine "$ENGINE" >"$KNOWLITH_HOME/dev.log" 2>&1 &
DAEMON=$!
trap stop_all INT TERM EXIT

wait_for_daemon || exit 1
say "log: $KNOWLITH_HOME/dev.log"

step 'Configure via curl (same fields as Settings + onboarding)'
api -X PUT -H 'Content-Type: application/json' \
  -d "{\"processing\":\"automatic\",\"pauseOnBattery\":false,\"largeScan\":500,\"engine\":\"$ENGINE\"}" \
  "$BASE/api/policy" >/dev/null

api -X PUT -H 'Content-Type: application/json' \
  -d "{\"name\":\"$COMPANY_NAME\",\"profile\":\"$COMPANY_PROFILE\"}" \
  "$BASE/api/company" >/dev/null

if [ "$WITH_DEMO" = "1" ]; then
  step 'Demo source'
  mkdir -p "$DEMO"
  cp "$ROOT/fixtures/knowlith-demo/invoices/"*.md "$DEMO/" 2>/dev/null || true
  api -X POST -H 'Content-Type: application/json' \
    -d "{\"path\":\"$DEMO\",\"name\":\"Demo invoices\",\"processor\":\"$ENGINE\"}" \
    "$BASE/api/sources" | python3 -m json.tool
fi

step 'Opening UI fullscreen'
open_fullscreen "$BASE"

printf '\n  Ctrl-C stops the daemon.\n'
printf '  Work panel: %s\n' "$BASE"
printf '  Build quiz: %s/build-quiz\n\n' "$BASE"

wait "$DAEMON"
