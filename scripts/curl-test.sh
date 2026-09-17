#!/bin/sh
# Smoke-test a running Knowlith daemon with curl.
#
#   sh scripts/curl-test.sh
#   KNOWLITH_PORT=7717 sh scripts/curl-test.sh
#
# Requires the daemon on 127.0.0.1 and a token at ~/Knowlith/api.token.

set -eu

PORT="${KNOWLITH_PORT:-7717}"
BASE="http://127.0.0.1:${PORT}"
TOKEN_FILE="${KNOWLITH_TOKEN_FILE:-$HOME/Knowlith/api.token}"

[ -f "$TOKEN_FILE" ] || {
  printf '\nno token at %s — start Knowlith first (knowlith start)\n\n' "$TOKEN_FILE" >&2
  exit 1
}

TOKEN=$(cat "$TOKEN_FILE")
HDR=(-H "x-knowlith-token: $TOKEN" -H "Content-Type: application/json")

curlq() {
  method=$1
  path=$2
  body=${3:-}
  if [ -n "$body" ]; then
    curl -sf "${HDR[@]}" -X "$method" -d "$body" "$BASE$path"
  else
    curl -sf "${HDR[@]}" -X "$method" "$BASE$path"
  fi
}

step() { printf '\n\033[1m%s\033[0m\n' "$*"; }

printf '\n\033[1mKnowlith — curl smoke test\033[0m\n'
printf '  %s\n' "$BASE"

step '1 · Health'
curlq GET /api/health | python3 -m json.tool

step '2 · Company'
curlq GET /api/company | python3 -m json.tool

step '3 · Sources'
curlq GET /api/sources | python3 -m json.tool

step '4 · Review queue'
curlq GET /api/review | python3 -c 'import json,sys; r=json.load(sys.stdin); print(f"  {len(r)} waiting")'

step '5 · Work queue'
curlq GET /api/work | python3 -m json.tool

step '6 · Policy'
curlq GET /api/policy | python3 -m json.tool

step '7 · Brain (graph)'
curlq GET /api/brain | python3 -c "import json,sys; d=json.load(sys.stdin); print('  nodes:', len(d.get('nodes', [])), ' edges:', len(d.get('edges', [])))"

step '8 · Connected AI tools'
curlq GET /api/tools | python3 -c "import json,sys
for t in json.load(sys.stdin):
 mark='connected' if t.get('connected') else 'not connected'
 print(' ', t.get('label', t.get('slug')), ':', mark)"

step '9 · Build supervisor'
curlq GET /api/build/status | python3 -m json.tool

step '10 · Build quiz (if any)'
curlq GET /api/build/quiz | python3 -c "import json,sys; q=json.load(sys.stdin)
print('  no quiz yet') if q is None else print('  state:', q.get('state'), ' questions:', len(q.get('questions', [])))"

step '11 · Engine runs (token spend when CLI reported it)'
curlq GET /api/engine-runs | python3 -c "import json,sys; r=json.load(sys.stdin); print(f'  {len(r)} runs logged')"

step '12 · Export approved knowledge to Markdown'
curlq POST /api/export '{}' | python3 -m json.tool

printf '\n\033[1mDone.\033[0m Full setup: sh scripts/curl-onboarding.sh --demo\n\n'
