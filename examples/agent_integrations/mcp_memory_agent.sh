#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
DB_PATH="${DB_PATH:-sqlrite_demo.db}"
AUTH_TOKEN="${AUTH_TOKEN:-mcp-demo-token}"
QUERY_TEXT="${QUERY_TEXT:-agent memory}"
TOP_K="${TOP_K:-2}"

emit_frame() {
  local body="$1"
  printf 'Content-Length: %d\r\n\r\n%s' "${#body}" "$body"
}

{
  emit_frame '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"mcp-example","version":"0.1"}}}'
  emit_frame '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
  emit_frame "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"search\",\"arguments\":{\"query_text\":\"${QUERY_TEXT}\",\"top_k\":${TOP_K},\"auth_token\":\"${AUTH_TOKEN}\"}}}"
} | "$BIN" mcp --db "$DB_PATH" --auth-token "$AUTH_TOKEN"
