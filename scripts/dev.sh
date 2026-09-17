#!/bin/sh
# Both halves of the product, one command, hot reload on the interface.
#
#   sh scripts/dev.sh            carry on from what is already there
#   sh scripts/dev.sh --fresh    start from an empty company
#
# The daemon is a debug build so a Rust change costs seconds, not minutes.
# The interface runs on Vite, so a React change is instant. They talk over
# 7717, which is the same address the shipped binary uses — the only
# difference here is that the interface is served by Vite instead of being
# baked into the binary.
#
# Vite forwards `/api` to the daemon rather than letting the browser reach
# it directly, and attaches the daemon's token on the way. So the page is
# same-origin here exactly as it is in the shipped binary, and the token
# never enters the browser at all.
#
# Ctrl-C stops both. Nothing is installed and nothing is registered to start
# at login: this is a workbench, not an install.
#
# `--fresh` moves the existing company aside rather than deleting it. A
# lake is the only copy of what somebody approved, and a testing flag is
# no reason to be the thing that loses it.

set -eu

cd "$(dirname "$0")/.."

FRESH=""
for arg in "$@"; do
  case "$arg" in
    --fresh) FRESH="yes" ;;
    *) printf '\nunknown option: %s\n\n' "$arg" >&2; exit 2 ;;
  esac
done

PORT="${KNOWLITH_PORT:-7717}"
UI_PORT="${KNOWLITH_UI_PORT:-5173}"
ROOT="${KNOWLITH_HOME:-$HOME/Knowlith}"

say() { printf '  %s\n' "$*"; }
die() { printf '\nfailed: %s\n\n' "$*" >&2; exit 1; }

printf '\n\033[1mKnowlith — development\033[0m\n\n'

command -v cargo >/dev/null 2>&1 || die 'Rust is not installed. https://rustup.rs'
command -v node  >/dev/null 2>&1 || die 'Node is not installed.'

lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1 \
  && die "something is already listening on $PORT — stop it, or set KNOWLITH_PORT"

[ -d knowlith/node_modules ] || { say 'installing interface dependencies'; (cd knowlith && npm ci >/dev/null 2>&1) || die 'npm ci failed'; }

if [ -n "$FRESH" ] && [ -d "$ROOT" ]; then
  ASIDE="$ROOT.$(date +%Y%m%d-%H%M%S)"
  mv "$ROOT" "$ASIDE"
  say "previous company moved to $ASIDE"
fi

mkdir -p "$ROOT"

# Both children die with this script, however it ends: a stray daemon holding
# 7717 is the one thing that makes the next run fail for no visible reason.
DAEMON=""
UI=""
stop() {
  trap - INT TERM EXIT
  [ -n "$UI" ]     && kill "$UI"     2>/dev/null || true
  [ -n "$DAEMON" ] && kill "$DAEMON" 2>/dev/null || true
  wait 2>/dev/null || true
  printf '\nstopped\n\n'
}
trap stop INT TERM EXIT

say 'building the daemon (debug — first time takes a minute)'
cargo build --bin knowlith >/dev/null 2>&1 || die 'the daemon did not build — run `cargo build --bin knowlith` to see why'

# Engine defaults to cursor-agent so a Settings change mid-session is not
# required to avoid auto picking Claude Code on a fresh lake.
ENGINE="${KNOWLITH_ENGINE:-cursor-agent}"

say "daemon   http://127.0.0.1:$PORT ($ENGINE)"
./target/debug/knowlith serve --port "$PORT" --engine "$ENGINE" >"$ROOT/dev.log" 2>&1 &
DAEMON=$!

# Given a moment, so a daemon that exits on startup is reported here rather
# than as a confusing connection error in the browser.
sleep 2
kill -0 "$DAEMON" 2>/dev/null || { printf '\n'; tail -20 "$ROOT/dev.log" >&2; die "the daemon exited — $ROOT/dev.log"; }

# Written by the daemon on first start. Vite reads it to talk to the API,
# so it has to be there before Vite is started, not merely soon after.
[ -f "$ROOT/api.token" ] || die "the daemon did not write $ROOT/api.token — see $ROOT/dev.log"

say "log      $ROOT/dev.log"
say "interface http://localhost:$UI_PORT"
printf '\n  Ctrl-C stops both.\n\n'

(cd knowlith && KNOWLITH_HOME="$ROOT" KNOWLITH_PORT="$PORT" npm run dev -- --port "$UI_PORT" --strictPort) &
UI=$!

wait "$UI"
