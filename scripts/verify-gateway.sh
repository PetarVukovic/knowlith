#!/bin/sh
# Asks the gateway the way an AI tool asks it.
#
#   sh scripts/verify-gateway.sh
#
# Speaks real JSON-RPC over the real binary's stdin, so what this prints is
# exactly what Claude or Codex would receive. If this works and the chat
# window does not, the problem is the connection, not Knowlith.

set -eu

cd "$(dirname "$0")/.."

BIN="${KNOWLITH_BIN:-$HOME/.local/bin/knowlith}"
# Read from the lake rather than assumed: the daemon, the gateway and the
# extension all take the name from there, and a default here would be the
# one place that disagrees.
COMPANY="${KNOWLITH_COMPANY:-}"
DB="${KNOWLITH_DB:-$HOME/Knowlith/data/lake.sqlite}"

[ -x "$BIN" ] || { printf '\nno knowlith at %s — run scripts/full-test.sh first\n\n' "$BIN" >&2; exit 1; }
[ -f "$DB" ]  || { printf '\nno lake at %s — run scripts/full-test.sh first\n\n' "$DB" >&2; exit 1; }

printf '\n\033[1mWhat an AI tool sees\033[0m\n'

ask() {
  # One conversation per question: initialize, then the call. A real client
  # holds the process open; this does not, which is the honest way to prove
  # that no state is being carried between calls.
  printf '%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"verify","version":"1"}}}' \
    "$1" \
  | { if [ -n "$COMPANY" ]; then
        "$BIN" --db "$DB" mcp --company "$COMPANY" 2>/dev/null
      else
        "$BIN" --db "$DB" mcp 2>/dev/null
      fi; }
}

render() {
  python3 -c '
import json, sys

for line in sys.stdin:
    message = json.loads(line)
    result = message.get("result", {})
    if "instructions" in result:
        continue
    if "content" in result:
        if result.get("isError"):
            print("  REFUSED:", result["content"][0]["text"])
            continue
        print("  " + result["content"][0]["text"].replace("\n", "\n  "))
        links = [c for c in result["content"] if c.get("type") == "resource_link"]
        if links:
            print("\n  sources it can open:", ", ".join(sorted({l["name"] for l in links})))
'
}

printf '\n\033[1m1 · "What can this company tell me about discounts?"\033[0m\n'
ask '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_context","arguments":{"question":"popust za stalne kupce"}}}' | render

printf '\n\033[1m2 · "What does installing a multi split cost?"\033[0m\n'
printf '  (read from the row, never recalled from a sentence)\n'
ask '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lookup_value","arguments":{"what":"montaža multi split"}}}' | render

printf '\n\033[1m3 · "What has the owner not decided?"\033[0m\n'
printf '  (the subject is named; neither answer is given)\n'
ask '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_pending","arguments":{}}}' | render

printf '\n\033[1m4 · Coverage — the part no retrieval system can do\033[0m\n'
CASE=$(ask '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_relevant_context","arguments":{"question":"pripremi ponudu za montažu klima uređaja stalnom kupcu"}}}' \
  | python3 -c '
import json, sys
for line in sys.stdin:
    result = json.loads(line).get("result", {})
    structured = result.get("structuredContent") or {}
    if "caseId" in structured:
        areas = structured.get("areas", [])
        print("  the company has decided", len(areas), "things that touch this:")
        for area in areas:
            print("    -", area["title"], "—", area["why"])
        for question in structured.get("openQuestions", []):
            print("    ! open:", question["subject"])
        print("CASE:" + (structured["caseId"] or ""))
')
printf '%s\n' "$CASE" | grep -v '^CASE:' || true
ID=$(printf '%s\n' "$CASE" | sed -n 's/^CASE://p')

if [ -n "$ID" ]; then
  printf '\n  Now closing that case without having read any of it:\n'
  ask "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"check_coverage\",\"arguments\":{\"caseId\":\"$ID\"}}}" | render
fi

printf '\n'
printf 'If sections 1 and 2 came back empty, nothing has been approved yet —\n'
printf 'open http://127.0.0.1:7717 and approve something, then run this again.\n\n'
