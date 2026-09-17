#!/bin/sh
# Configure a fresh Knowlith daemon via curl — same fields as Settings + onboarding.
#
# Prerequisite: daemon already running and ~/Knowlith/api.token exists.
#
#   cargo build -p knowlith-cli
#   ./target/debug/knowlith serve --engine cursor-agent &
#   sh scripts/curl-onboarding.sh --demo
#
# With Medicor demo on Desktop:
#   KNOWLITH_DEMO=~/Desktop/Medicor-Klinika-Demo \
#   KNOWLITH_COMPANY="Medicor poliklinika" \
#   sh scripts/curl-onboarding.sh --demo

set -eu

PORT="${KNOWLITH_PORT:-7717}"
BASE="http://127.0.0.1:${PORT}"
TOKEN_FILE="${KNOWLITH_TOKEN_FILE:-$HOME/Knowlith/api.token}"
ENGINE="${KNOWLITH_ENGINE:-cursor-agent}"
COMPANY_NAME="${KNOWLITH_COMPANY:-Demo}"
COMPANY_PROFILE="${KNOWLITH_PROFILE:-We send invoices to Formify. Bethel AI issues them in EUR with net-7 payment terms.}"
DEMO="${KNOWLITH_DEMO:-$HOME/Documents/knowlith-demo/invoices}"

WITH_DEMO=0
for arg in "$@"; do
  case "$arg" in
    --demo) WITH_DEMO=1 ;;
    *) printf 'unknown option: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

[ -f "$TOKEN_FILE" ] || {
  printf '\nno token at %s — start the daemon first\n\n' "$TOKEN_FILE" >&2
  exit 1
}

TOKEN=$(cat "$TOKEN_FILE")
HDR=(-H "x-knowlith-token: $TOKEN" -H "Content-Type: application/json")

api() {
  curl -sf "${HDR[@]}" "$@"
}

step() { printf '\n\033[1m%s\033[0m\n' "$*"; }

wait_for_daemon() {
  for _ in $(seq 1 60); do
    if api "$BASE/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  printf 'daemon not healthy at %s\n' "$BASE" >&2
  return 1
}

printf '\n\033[1mKnowlith — curl onboarding\033[0m\n'
printf '  %s\n' "$BASE"

step 'Wait for health'
wait_for_daemon
api "$BASE/api/health" | python3 -m json.tool

step 'Policy (automatic, no battery hold, relate after build quiz)'
api -X PUT -d "{\"processing\":\"automatic\",\"pauseOnBattery\":false,\"largeScan\":500,\"engine\":\"$ENGINE\",\"compileWorkers\":2,\"compileBatchSize\":8,\"relateAfterBuild\":true}" \
  "$BASE/api/policy" | python3 -m json.tool

step 'Company'
api -X PUT -d "{\"name\":\"$COMPANY_NAME\",\"profile\":\"$COMPANY_PROFILE\"}" \
  "$BASE/api/company" | python3 -m json.tool

if [ "$WITH_DEMO" = "1" ]; then
  step 'Demo source'
  case "$DEMO" in
    "$HOME/Documents/knowlith-demo/invoices")
      mkdir -p "$DEMO"
      ROOT="$(cd "$(dirname "$0")/.." && pwd)"
      cp "$ROOT/fixtures/knowlith-demo/invoices/"*.md "$DEMO/" 2>/dev/null || true
      ;;
  esac
  printf '  waiting for write-settle…\n'
  sleep 4
  SOURCE=$(api -X POST -d "{\"path\":\"$DEMO\",\"name\":\"Demo source\",\"processor\":\"$ENGINE\"}" "$BASE/api/sources")
  echo "$SOURCE" | python3 -m json.tool
  SOURCE_ID=$(echo "$SOURCE" | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")
  printf '  source id: %s\n' "$SOURCE_ID"

  step 'Poll until documents land'
  for i in $(seq 1 24); do
    HEALTH=$(api "$BASE/api/health")
    DOCS=$(echo "$HEALTH" | python3 -c "import json,sys; print(json.load(sys.stdin).get('documents',0))")
    WORK=$(api "$BASE/api/work")
    STAGE=$(echo "$WORK" | python3 -c "import json,sys; print(json.load(sys.stdin).get('stage',''))")
    printf '  [%s] documents=%s stage=%s\n' "$i" "$DOCS" "$STAGE"
    if [ "$DOCS" -gt 0 ]; then
      break
    fi
    if [ "$STAGE" = "idle" ] && [ "$DOCS" -eq 0 ] && [ "$i" -eq 8 ]; then
      printf '  re-queueing rescan\n'
      api -X POST "$BASE/api/sources/$SOURCE_ID/rescan" >/dev/null
    fi
    sleep 5
  done
fi

step 'Build status'
api "$BASE/api/build/status" | python3 -m json.tool

step 'Work panel'
api "$BASE/api/work" | python3 -m json.tool

printf '\n\033[1mOnboarding curl done.\033[0m\n'
printf '  UI:        %s\n' "$BASE"
printf '  Build quiz: %s/build-quiz\n' "$BASE"
printf '  Smoke test: sh scripts/curl-test.sh\n\n'
