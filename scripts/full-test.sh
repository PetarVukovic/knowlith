#!/bin/sh
# One full run, from an empty machine to an AI tool answering from a
# company's own documents.
#
#   sh scripts/full-test.sh                        the test company, no cost
#   sh scripts/full-test.sh ~/Documents/MojaFirma  your folder, your CLI
#
# With no argument it replays recorded engine replies, so the whole compile
# runs with no provider, no key and no cost, and produces the same numbers
# every time — which is what makes it a test rather than a demo.
#
# With a folder it uses whichever AI command line is installed on this
# machine, on your own subscription, and it will cost whatever that costs.
# It says so and asks first.
#
# Either way it stops before approving anything. Nothing becomes this
# company's knowledge without the owner, so the last step opens the
# interface and hands it over.

set -eu

cd "$(dirname "$0")/.."
REPO="$(pwd)"

BIN_DIR="${KNOWLITH_BIN_DIR:-$HOME/.local/bin}"
BIN="$BIN_DIR/knowlith"
PORT="${KNOWLITH_PORT:-7717}"
ROOT="${KNOWLITH_HOME:-$HOME/Knowlith}"

FOLDER="${1:-}"
if [ -n "$FOLDER" ]; then
  MODE=own
  FOLDER=$(cd "$FOLDER" 2>/dev/null && pwd) || { printf '\nthere is no folder at %s\n\n' "$1" >&2; exit 1; }
  COMPANY="${KNOWLITH_COMPANY:-$(basename "$FOLDER")}"
else
  MODE=fixture
  FOLDER="$REPO/fixtures/termoval/source/Termoval - Prodaja"
  COMPANY="${KNOWLITH_COMPANY:-Termoval d.o.o.}"
fi

say()  { printf '  %s\n' "$*"; }
step() { printf '\n\033[1m%s\033[0m\n' "$*"; }
die()  { printf '\nfailed: %s\n\n' "$*" >&2; exit 1; }
ask()  { printf '  %s [y/N] ' "$1"; read -r reply; case "$reply" in y|Y|yes) return 0 ;; *) return 1 ;; esac }

printf '\n\033[1mKnowlith — one full run\033[0m\n'

# ------------------------------------------------------- 0. what is needed --

step '0 · What this needs'
command -v cargo >/dev/null 2>&1 || die 'Rust is not installed. https://rustup.rs'
command -v node  >/dev/null 2>&1 || die 'Node is not installed — the interface is built with it.'
say "rust $(rustc --version | awk '{print $2}')  ·  node $(node --version)"
[ -d "$FOLDER" ] || die "there is no folder at $FOLDER"

FILES=$(find "$FOLDER" -type f 2>/dev/null | wc -l | tr -d ' ')
say "company: $COMPANY"
say "folder:  $FOLDER  ($FILES files)"
if [ "$MODE" = fixture ]; then
  say 'engine:  recorded replies — no provider, no cost'
else
  say 'engine:  whichever AI command line is on this machine, on your subscription'
fi

[ -e "$ROOT/data/lake.sqlite" ] && die "there is already a lake at $ROOT. Run: sh scripts/reset.sh --yes"

# ----------------------------------------------------------- 1. the build --

step '1 · Build'
say 'interface'
if [ -d knowlith/node_modules ]; then
  (cd knowlith && npm run build >/dev/null 2>&1) || die 'the interface did not build'
else
  (cd knowlith && npm ci >/dev/null 2>&1 && npm run build >/dev/null 2>&1) || die 'the interface did not build'
fi
say "  $(du -sh knowlith/dist | cut -f1) of interface"

say 'daemon (release — a few minutes the first time)'
cargo build --release --bin knowlith >/dev/null 2>&1 || die 'the daemon did not build'
mkdir -p "$BIN_DIR"
cp target/release/knowlith "$BIN"
chmod +x "$BIN"
say "  $("$BIN" --version) → $BIN  ($(du -h "$BIN" | cut -f1), interface included)"

if [ "$MODE" = own ]; then
  step 'Which AI command line is installed'
  "$BIN" engines 2>&1 | sed 's/^/  /'
fi

# ------------------------------------------------------------ 2. the read --

step '2 · Read the folder'
say 'no model involved — extraction is deterministic, the same bytes always'
say 'give the same text and the same byte offsets'
"$BIN" scan "$FOLDER" --source main 2>&1 | sed 's/^/  /'

# --------------------------------------------------------- 3. the compile --

step '3 · Turn it into knowledge'
if [ "$MODE" = fixture ]; then
  "$BIN" work --replay "$REPO/fixtures/termoval/cassettes" --once 2>&1 | tail -10 | sed 's/^/  /'
else
  printf '\n'
  say "This reads $FILES files with your own AI command line, one call per"
  say 'document. It runs on your subscription and it is the only step here'
  say 'that costs anything.'
  printf '\n'
  ask 'Go ahead?' || { say 'stopped — the folder is read, nothing was compiled'; say "run `knowlith work` when you want to"; exit 0; }
  printf '\n'
  "$BIN" work --once 2>&1 | sed 's/^/  /'
fi

step '4 · What came out'
"$BIN" status 2>&1 | sed 's/^/  /'
printf '\n'
say 'Where two of the documents say different things:'
"$BIN" merges 2>&1 | head -14 | sed 's/^/    /'

# --------------------------------------------------- 5. the AI applications --

step '5 · Hand it to the AI tools on this machine'
"$BIN" connect --dry-run 2>&1 | sed 's/^/    /'
printf '\n'
say 'Every other server in those files is kept, and a copy of each file is'
say 'saved beside it first.'
if ask 'Write it?'; then
  "$BIN" connect --company "$COMPANY" 2>&1 | sed 's/^/    /'
else
  say 'skipped — `knowlith connect` does it later'
fi

# ------------------------------------------------------- 6. the interface --

step '6 · Open the interface'
lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1 && die "something is already listening on $PORT"

# The worker is left on: closing this window should not stop the reading,
# and that is the claim being tested.
"$BIN" serve --port "$PORT" --company "$COMPANY" >"$ROOT/serve.log" 2>&1 &
SERVE_PID=$!
sleep 2
kill -0 "$SERVE_PID" 2>/dev/null || die "the daemon exited — see $ROOT/serve.log"
say "running as process $SERVE_PID on http://127.0.0.1:$PORT"
say "log: $ROOT/serve.log"
command -v open >/dev/null 2>&1 && open "http://127.0.0.1:$PORT" || true

# ------------------------------------------------------------ 7. over to you --

PENDING=$("$BIN" status 2>/dev/null | sed -n 's/.*· \([0-9]*\) waiting for review.*/\1/p')

cat <<INSTRUCTIONS

$(printf '\033[1m7 · Your turn\033[0m')

  Nothing has been approved, and nothing will be without you. That is the
  product, so the test stops here.

  In the window that just opened:

    1. Review — ${PENDING:-some} things are waiting. Approve a few.
    2. Where it says two documents disagree, it shows you both sentences and
       which file each came from, and asks you to pick.
    3. Approve a process and a skill gets drafted from it, in the background,
       without you asking.

  Then check that an AI tool really sees it:

    sh scripts/verify-gateway.sh

  And in Claude Desktop or Codex:

    What discount can we approve for a regular customer?

  The answer has to name the document. If it does not, the tool is not
  reading $COMPANY.

  Stop it:   kill $SERVE_PID
  Undo all:  sh scripts/reset.sh --yes

INSTRUCTIONS
