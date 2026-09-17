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

SOURCE_ID=""
if [ "$WITH_DEMO" = "1" ]; then
  step 'Demo source'
  # Default demo path gets fixture invoices copied in; a custom KNOWLITH_DEMO
  # folder (e.g. a full SMB tree on Desktop) is used as-is.
  case "$DEMO" in
    "$HOME/Documents/knowlith-demo/invoices")
      mkdir -p "$DEMO"
      cp "$ROOT/fixtures/knowlith-demo/invoices/"*.md "$DEMO/" 2>/dev/null || true
      ;;
  esac
  # Rescan skips files whose mtime is <3s old (write-settle). Copying here
  # and posting the source in the same second leaves every file "still being
  # written" and the lake stays at zero documents.
  say "waiting for write-settle on copied files…"
  sleep 4
  SOURCE_ID=$(api -X POST -H 'Content-Type: application/json' \
    -d "{\"path\":\"$DEMO\",\"name\":\"Demo invoices\",\"processor\":\"$ENGINE\"}" \
    "$BASE/api/sources" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  say "source id: $SOURCE_ID"

  step 'Waiting for documents to land in the lake'
  for i in $(seq 1 24); do
    HEALTH=$(api "$BASE/api/health")
    DOCS=$(echo "$HEALTH" | python3 -c "import json,sys; print(json.load(sys.stdin).get('documents',0))")
    WORK=$(api "$BASE/api/work")
    STAGE=$(echo "$WORK" | python3 -c "import json,sys; print(json.load(sys.stdin).get('stage',''))")
    DOING=$(echo "$WORK" | python3 -c "import json,sys; print(json.load(sys.stdin).get('doing',''))")
    say "[$i] documents=$DOCS stage=$STAGE — $DOING"
    if [ "$DOCS" -gt 0 ]; then
      break
    fi
    if [ "$STAGE" = "idle" ] && [ "$DOCS" -eq 0 ] && [ "$i" -eq 8 ]; then
      say "re-queueing rescan (first pass may have hit write-settle)"
      api -X POST "$BASE/api/sources/$SOURCE_ID/rescan" >/dev/null
    fi
    sleep 5
  done
fi

step 'Opening UI fullscreen'
OPEN_URL="$BASE/onboarding"
[ "$FRESH" = "1" ] || OPEN_URL="$BASE"
open_fullscreen "$OPEN_URL"
say "compile and the build supervisor continue in the background — watch Work or /build-quiz"

printf '\n  Ctrl-C stops the daemon.\n'
printf '  Work panel: %s\n' "$BASE"
printf '  Build quiz: %s/build-quiz\n\n' "$BASE"

wait "$DAEMON"
