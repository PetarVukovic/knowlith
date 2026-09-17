#!/bin/sh
# Wipe local Knowlith, rebuild, run the supervisor demo end-to-end, then start UI.
#
#   sh scripts/fresh-start.sh
#
# Needs a CLI agent on PATH (default: cursor-agent). Override:
#   KNOWLITH_ENGINE=codex sh scripts/fresh-start.sh
#
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> Stopping anything on 7717 / 5173"
pkill -f 'knowlith serve' 2>/dev/null || true
pkill -f 'vite.*knowlith' 2>/dev/null || true
sleep 1

echo "==> Wiping local install"
sh "$ROOT/scripts/reset.sh" --yes

echo "==> Supervisor build test (demo invoices + live CLI agent)"
export KNOWLITH_ENGINE="${KNOWLITH_ENGINE:-cursor-agent}"
export KNOWLITH_DEMO="${KNOWLITH_DEMO:-$HOME/Documents/knowlith-demo}"
sh "$ROOT/scripts/test-supervisor-build.sh"

echo ""
echo "==> Starting Knowlith (daemon + embedded UI + browser)"
exec "$ROOT/target/debug/knowlith" start --engine "$KNOWLITH_ENGINE"
