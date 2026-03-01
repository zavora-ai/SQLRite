#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
DB_PATH="${DB_PATH:-project_plan/reports/s20_mcp_smoke.db}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s20_mcp_smoke.log}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s20-token}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" /tmp/s20_mcp_raw.bin /tmp/s20_mcp_raw.txt

"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s20_init.log 2>&1

echo "[manifest]" | tee -a "$LOG_PATH"
"$BIN" mcp --db "$DB_PATH" --auth-token "$CONTROL_TOKEN" --print-manifest | tee -a "$LOG_PATH"

emit_frame() {
  local body="$1"
  printf 'Content-Length: %d\r\n\r\n%s' "${#body}" "$body"
}

{
  emit_frame '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"s20-smoke","version":"0.1"}}}'
  emit_frame '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
  emit_frame '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  emit_frame '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"health","arguments":{"auth_token":"s20-token"}}}'
  emit_frame '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search","arguments":{"query_text":"local memory","top_k":2,"auth_token":"s20-token"}}}'
  emit_frame '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"health","arguments":{}}}'
} | "$BIN" mcp --db "$DB_PATH" --auth-token "$CONTROL_TOKEN" > /tmp/s20_mcp_raw.bin

tr '\r' '\n' < /tmp/s20_mcp_raw.bin > /tmp/s20_mcp_raw.txt

echo "\n[mcp-raw-responses]" | tee -a "$LOG_PATH"
cat /tmp/s20_mcp_raw.txt | tee -a "$LOG_PATH" >/dev/null

echo "\n[assertions]" | tee -a "$LOG_PATH"
grep -q '"protocolVersion":"2024-11-05"' /tmp/s20_mcp_raw.txt
echo "initialize_protocol_version=ok" | tee -a "$LOG_PATH"
grep -q '"name":"search"' /tmp/s20_mcp_raw.txt
echo "tools_list_contains_search=ok" | tee -a "$LOG_PATH"
grep -q '"isError":false' /tmp/s20_mcp_raw.txt
echo "authorized_tool_call=ok" | tee -a "$LOG_PATH"
grep -q '"code":-32001' /tmp/s20_mcp_raw.txt
echo "unauthorized_tool_call_rejected=ok" | tee -a "$LOG_PATH"

echo "\n[s20-mcp-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"

if [[ "$KEEP_DB" != "1" ]]; then
  rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
fi
