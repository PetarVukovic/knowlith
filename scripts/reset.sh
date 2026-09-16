#!/bin/sh
# Puts this machine back to before Knowlith was ever installed.
#
#   sh scripts/reset.sh          show what would go, remove nothing
#   sh scripts/reset.sh --yes    do it
#
# Everything here is reversible except the lake, and the lake is rebuilt by
# the next scan. Nothing outside Knowlith's own files is touched: the entry
# is taken out of each application's configuration and every other server in
# those files stays exactly where it is.

set -eu

DO_IT=0
[ "${1:-}" = "--yes" ] && DO_IT=1

BIN="${KNOWLITH_BIN:-$HOME/.local/bin/knowlith}"
ROOT="${KNOWLITH_HOME:-$HOME/Knowlith}"

say()  { printf '  %s\n' "$*"; }
step() { printf '\n%s\n' "$*"; }
run()  { if [ "$DO_IT" = "1" ]; then "$@"; else say "would run: $*"; fi }

printf '\nKnowlith — reset\n'
[ "$DO_IT" = "1" ] || say '(dry run — nothing will be changed; add --yes to do it)'

# --------------------------------------------------------- stop what runs --

step 'Running processes'
if lsof -nP -iTCP:7717 -sTCP:LISTEN >/dev/null 2>&1; then
  say 'something is listening on 7717'
  if [ "$DO_IT" = "1" ]; then
    pkill -f 'knowlith serve' 2>/dev/null || true
    sleep 1
  fi
else
  say 'nothing on 7717'
fi

# ---------------------------------------------------------- login service --

step 'Login service'
if [ -x "$BIN" ]; then
  run "$BIN" autostart off
else
  # The plist is removed by hand when the binary is already gone, otherwise
  # launchd keeps trying to start something that is not there.
  PLIST="$HOME/Library/LaunchAgents/eu.knowlith.agent.plist"
  if [ -f "$PLIST" ]; then
    run launchctl bootout "gui/$(id -u)" "$PLIST"
    run rm -f "$PLIST"
  else
    say 'not registered'
  fi
fi

# ------------------------------------------------------- AI applications --

step 'AI applications'
if [ -x "$BIN" ]; then
  run "$BIN" disconnect
else
  say "no binary at $BIN — removing the entries directly"
  for f in "$HOME/Library/Application Support/Claude/claude_desktop_config.json" "$HOME/.claude.json"; do
    [ -f "$f" ] || continue
    if grep -q '"knowlith"' "$f" 2>/dev/null; then
      say "knowlith is in $f"
      if [ "$DO_IT" = "1" ]; then
        cp "$f" "$f.pre-reset"
        python3 - "$f" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path))
data.get("mcpServers", {}).pop("knowlith", None)
open(path, "w").write(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
PY
      fi
    fi
  done
  CODEX="$HOME/.codex/config.toml"
  if [ -f "$CODEX" ] && grep -q '^\[mcp_servers\.knowlith\]' "$CODEX"; then
    say "knowlith is in $CODEX"
    if [ "$DO_IT" = "1" ]; then
      cp "$CODEX" "$CODEX.pre-reset"
      python3 - "$CODEX" <<'PY'
import sys
path = sys.argv[1]
out, dropping = [], False
for line in open(path).read().split("\n"):
    # Only a table header starts a line with "[" at column zero; `args = [`
    # does not, and neither do its continuation lines.
    if line.startswith("["):
        dropping = line.strip() == "[mcp_servers.knowlith]"
    if not dropping:
        out.append(line)
open(path, "w").write("\n".join(out))
PY
    fi
  fi
fi

# --------------------------------------------------- standing instructions --

step 'Standing instructions'
for f in "$HOME/.codex/AGENTS.md" "$HOME/.claude/CLAUDE.md"; do
  if [ -f "$f" ] && grep -q 'knowlith:begin' "$f" 2>/dev/null; then
    say "a Knowlith block is in $f"
    if [ "$DO_IT" = "1" ]; then
      cp "$f" "$f.pre-reset"
      python3 - "$f" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
start = text.find("<!-- knowlith:begin -->")
end = text.find("<!-- knowlith:end -->")
# An unterminated block is left alone rather than swallowing the rest of
# somebody's instructions file.
if start != -1 and end > start:
    end += len("<!-- knowlith:end -->")
    open(path, "w").write((text[:start].rstrip() + "\n\n" + text[end:].lstrip()).strip() + "\n")
PY
    fi
  else
    say "nothing of ours in $f"
  fi
done

# ------------------------------------------------------------- the files --

step 'Files'
if [ -d "$ROOT" ]; then
  say "$ROOT ($(du -sh "$ROOT" 2>/dev/null | cut -f1))"
  run rm -rf "$ROOT"
else
  say "$ROOT does not exist"
fi

if [ -f "$BIN" ]; then
  say "$BIN"
  run rm -f "$BIN"
else
  say "no binary at $BIN"
fi

printf '\n'
if [ "$DO_IT" = "1" ]; then
  printf 'Gone. Run scripts/full-test.sh for a clean install.\n\n'
else
  printf 'Nothing was changed. Run it again with --yes.\n\n'
fi
