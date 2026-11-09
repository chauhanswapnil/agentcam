#!/usr/bin/env bash
# Smoke test for an installed agentcam binary, as a new user would run it.
#
#   scripts/smoke.sh [path/to/agentcam]
#
# Uses only the synthetic logs in fixtures/ (via CLAUDE_CONFIG_DIR and
# CODEX_HOME, so it works the same on macOS, Linux and Windows) and a
# scratch index. Never touches your real logs or index.
set -euo pipefail

BIN=${1:-agentcam}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
PORT=47${RANDOM: -2}
SERVER=
cleanup() {
  [ -n "$SERVER" ] && kill "$SERVER" 2>/dev/null || true
  rm -rf "$T"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

mkdir -p "$T/claude" "$T/codex"
cp -R "$ROOT/fixtures/claude-code/projects" "$T/claude/"
cp -R "$ROOT/fixtures/codex/sessions" "$T/codex/"
export CLAUDE_CONFIG_DIR="$T/claude" CODEX_HOME="$T/codex" AGENTCAM_DB="$T/index.db"

echo "== version"
want=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
"$BIN" --version | grep -q "agentcam $want" || fail "version is not $want"

echo "== index"
"$BIN" index | tee "$T/index.txt"
grep -q "/Users/dev/projects" "$T/index.txt" || fail "index found no fixture sessions"

echo "== search"
"$BIN" search login | grep -q "#" || fail "search found nothing for 'login'"

echo "== guard"
call='{"tool_name":"Bash","tool_input":{"command":"git push --force origin main"}}'
echo "$call" | "$BIN" guard || fail "guard blocked with no rules"
cat > "$T/guard.yaml" <<'EOF'
- id: no-force-push
  tool: Bash
  pattern: '\bgit\s+push\b.*--force'
  reason: Check with the user first.
EOF
set +e
echo "$call" | "$BIN" guard 2>"$T/guard.txt"
code=$?
set -e
[ "$code" = 2 ] || fail "guard exit $code, want 2"
grep -q "Check with the user first" "$T/guard.txt" || fail "guard gave no reason"
echo '{"tool_name":"Bash","tool_input":{"command":"ls"}}' | "$BIN" guard || fail "guard blocked a harmless call"
echo "not json" | "$BIN" guard || fail "guard must fail open on bad input"

echo "== serve"
"$BIN" serve --no-open --port "$PORT" >"$T/serve.log" 2>&1 &
SERVER=$!
url=
for _ in $(seq 1 60); do
  url=$(grep -o 'http://127.0.0.1:[0-9]*' "$T/serve.log" | tail -1 || true)
  if [ -n "$url" ] && curl -fsS "$url/api/status" 2>/dev/null | grep -q '"has_sessions":true'; then
    break
  fi
  url=
  sleep 0.5
done
[ -n "$url" ] || { cat "$T/serve.log"; fail "server never reported sessions"; }
curl -fsS -D "$T/headers.txt" "$url/" -o "$T/index.html"
grep -q 'assets/index-' "$T/index.html" || fail "the binary has the placeholder UI; build web/ first"
grep -qi 'frame-ancestors' "$T/headers.txt" || fail "page has no frame policy"
status=$(curl -s -o /dev/null -w '%{http_code}' -H 'Host: evil.example' "$url/api/status")
[ "$status" = 403 ] || fail "a non-loopback Host got $status"

echo "OK: $("$BIN" --version)"
