#!/bin/sh
# End-to-end build supervisor test with cursor-agent (or codex/claude).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KNOWLITH_HOME="${KNOWLITH_HOME:-$HOME/Knowlith}"
DEMO="${KNOWLITH_DEMO:-$HOME/Documents/knowlith-demo}"
TOKEN_FILE="$KNOWLITH_HOME/api.token"
LAKE="$KNOWLITH_HOME/data/lake.sqlite"
BASE=http://127.0.0.1:7717

ENGINE="${KNOWLITH_ENGINE:-cursor-agent}"

api() {
  if [ -f "$TOKEN_FILE" ]; then
    curl -sf -H "x-knowlith-token: $(cat "$TOKEN_FILE")" "$@"
  else
    curl -sf "$@"
  fi
}

wait_for_daemon() {
  for _ in $(seq 1 60); do
    if [ -f "$TOKEN_FILE" ] && api "$BASE/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "daemon never became healthy" >&2
  echo "serve log:" >&2
  tail -30 /tmp/knowlith-serve.log >&2 || true
  return 1
}

echo "==> Preparing demo folder at $DEMO"
mkdir -p "$DEMO/invoices"
cp "$ROOT/fixtures/knowlith-demo/invoices/"*.md "$DEMO/invoices/"

echo "==> Building knowlith"
cargo build -q --manifest-path "$ROOT/Cargo.toml" -p knowlith-cli

KNOWLITH="$ROOT/target/debug/knowlith"
export KNOWLITH_REPO="$ROOT"

echo "==> Fresh lake + policy (engine=$ENGINE)"
if [ -d "$KNOWLITH_HOME" ]; then
  ASIDE="$KNOWLITH_HOME.$(date +%Y%m%d-%H%M%S)"
  mv "$KNOWLITH_HOME" "$ASIDE"
  echo "    previous company moved to $ASIDE"
fi
mkdir -p "$KNOWLITH_HOME"

"$KNOWLITH" serve --engine "$ENGINE" >/tmp/knowlith-serve.log 2>&1 &
SERVE_PID=$!
wait_for_daemon || {
  kill "$SERVE_PID" 2>/dev/null || true
  exit 1
}

api -X PUT -H 'Content-Type: application/json' \
  -d "{\"processing\":\"automatic\",\"pauseOnBattery\":false,\"largeScan\":500,\"engine\":\"$ENGINE\"}" \
  "$BASE/api/policy" >/dev/null

api -X PUT -H 'Content-Type: application/json' \
  -d '{"profile":"We send invoices to Formify. Bethel AI issues them in EUR with net-7 payment terms."}' \
  "$BASE/api/company" >/dev/null

echo "==> Adding source"
api -X POST -H 'Content-Type: application/json' \
  -d "{\"path\":\"$DEMO/invoices\",\"name\":\"Demo invoices\",\"processor\":\"$ENGINE\"}" \
  "$BASE/api/sources"

echo ""
echo "==> Draining queue (may take several minutes with a live CLI)…"
for i in $(seq 1 120); do
  WORK=$(api "$BASE/api/work")
  STAGE=$(echo "$WORK" | python3 -c "import sys,json; print(json.load(sys.stdin).get('stage',''))")
  DONE=$(echo "$WORK" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('done',0), d.get('total',0))")
  echo "  [$i] stage=$STAGE $DONE"
  if [ "$STAGE" = "idle" ]; then
    BUILD=$(api "$BASE/api/build/status")
    PHASE=$(echo "$BUILD" | python3 -c "import sys,json; print(json.load(sys.stdin).get('phase',''))")
    echo "  build phase=$PHASE"
    if [ "$PHASE" = "quiz_pending" ] || [ "$PHASE" = "complete" ]; then
      break
    fi
  fi
  sleep 5
done

echo ""
echo "==> Build status"
api "$BASE/api/build/status" | python3 -m json.tool

echo ""
echo "==> Quiz (first question)"
api "$BASE/api/build/quiz" | python3 -c "
import sys, json
q = json.load(sys.stdin)
if not q:
    print('no quiz yet')
    sys.exit(0)
print('quiz', q.get('id'), 'questions', len(q.get('questions', [])))
for item in (q.get('questions') or [])[:3]:
    print('-', item.get('question'))
    print(' ', item.get('agentAnswer','')[:120])
"

echo ""
echo "==> Entities"
api "$BASE/api/build/entities" | python3 -m json.tool

kill "$SERVE_PID" 2>/dev/null || true
echo ""
echo "Done. Open http://127.0.0.1:7717/build-quiz after starting knowlith start"
