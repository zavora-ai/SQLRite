#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
DB_PATH="${DB_PATH:-project_plan/reports/s25_agent_contract.db}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8333}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s25_agent_contract_suite.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s25_agent_contract_report.json}"
SETUP_LOG_PATH="${SETUP_LOG_PATH:-project_plan/reports/s25_agent_memory_setup.log}"
SETUP_REPORT_PATH="${SETUP_REPORT_PATH:-project_plan/reports/s25_agent_memory_setup.json}"
MCP_TOKEN="${MCP_TOKEN:-s25-token}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" "$REPORT_PATH" \
  /tmp/s25_http_query.json /tmp/s25_bridge_query.json /tmp/s25_python_query.json /tmp/s25_ts_query.json \
  /tmp/s25_mcp_raw.bin /tmp/s25_mcp_query.json

echo "[setup-under-15m]" | tee -a "$LOG_PATH"
KEEP_DB=1 DB_PATH="$DB_PATH" BIND_ADDR="127.0.0.1:8332" LOG_PATH="$SETUP_LOG_PATH" REPORT_PATH="$SETUP_REPORT_PATH" \
  bash "$ROOT_DIR/scripts/run-s25-agent-memory-setup.sh" | tee -a "$LOG_PATH"

echo "[start-http-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve --db "$DB_PATH" --bind "$BIND_ADDR" >/tmp/sqlrite_s25_contract_server.log 2>&1 &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_DB" != "1" ]]; then
    rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
  fi
}
trap cleanup EXIT

for _ in $(seq 1 80); do
  if curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "server did not become ready" | tee -a "$LOG_PATH"
  tail -n 80 /tmp/sqlrite_s25_contract_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[http-query]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":2}' \
  "http://$BIND_ADDR/v1/query" > /tmp/s25_http_query.json

echo "[grpc-bridge-query]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":2}' \
  "http://$BIND_ADDR/grpc/sqlrite.v1.QueryService/Query" > /tmp/s25_bridge_query.json

echo "[python-sdk-query]" | tee -a "$LOG_PATH"
PYTHONPATH="$ROOT_DIR/sdk/python" python3 - <<'PY' "$BIND_ADDR" > /tmp/s25_python_query.json
import json
import sys
from sqlrite_sdk import SqlRiteClient
addr = sys.argv[1]
client = SqlRiteClient(f"http://{addr}")
payload = client.query(query_text="agent memory", top_k=2)
print(json.dumps(payload))
PY

echo "[typescript-sdk-query]" | tee -a "$LOG_PATH"
npm --prefix "$ROOT_DIR/sdk/typescript" install >/tmp/s25_contract_npm_install.log 2>&1
npm --prefix "$ROOT_DIR/sdk/typescript" run build >/tmp/s25_contract_npm_build.log 2>&1
node - <<'NODE' "$BIND_ADDR" > /tmp/s25_ts_query.json
import { SqlRiteClient } from './sdk/typescript/dist/index.js';
const addr = process.argv[2];
const client = new SqlRiteClient(`http://${addr}`);
const payload = await client.query({ query_text: 'agent memory', top_k: 2 });
console.log(JSON.stringify(payload));
NODE

echo "[mcp-query]" | tee -a "$LOG_PATH"
emit_frame() {
  local body="$1"
  printf 'Content-Length: %d\r\n\r\n%s' "${#body}" "$body"
}

{
  emit_frame '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"s25-contract","version":"0.1"}}}'
  emit_frame '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
  emit_frame "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"search\",\"arguments\":{\"query_text\":\"agent memory\",\"top_k\":2,\"auth_token\":\"${MCP_TOKEN}\"}}}"
} | "$BIN" mcp --db "$DB_PATH" --auth-token "$MCP_TOKEN" > /tmp/s25_mcp_raw.bin

python3 - <<'PY' /tmp/s25_mcp_raw.bin /tmp/s25_mcp_query.json
import json
import sys

raw = open(sys.argv[1], "rb").read()
offset = 0
responses = []
while offset < len(raw):
    marker = raw.find(b"\r\n\r\n", offset)
    if marker == -1:
        break
    header_blob = raw[offset:marker].decode("utf-8", errors="ignore")
    content_length = None
    for line in header_blob.split("\r\n"):
        if line.lower().startswith("content-length:"):
            content_length = int(line.split(":", 1)[1].strip())
            break
    if content_length is None:
        offset = marker + 4
        continue
    payload_start = marker + 4
    payload_end = payload_start + content_length
    if payload_end > len(raw):
        break
    payload = raw[payload_start:payload_end]
    try:
        responses.append(json.loads(payload.decode("utf-8")))
    except Exception:
        pass
    offset = payload_end

search = None
for response in responses:
    if response.get("id") == 2 and isinstance(response.get("result"), dict):
        search = response["result"].get("structuredContent")
        break

if not isinstance(search, list):
    raise SystemExit("mcp search structuredContent missing")

payload = {
    "kind": "query",
    "row_count": len(search),
    "rows": search,
}
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(payload, handle)
PY

echo "[contract-assertions]" | tee -a "$LOG_PATH"
python3 - <<'PY' \
  /tmp/s25_http_query.json \
  /tmp/s25_bridge_query.json \
  /tmp/s25_python_query.json \
  /tmp/s25_ts_query.json \
  /tmp/s25_mcp_query.json \
  "$SETUP_REPORT_PATH" \
  "$REPORT_PATH" \
  | tee -a "$LOG_PATH"
import hashlib
import json
import sys
import time

surface_paths = {
    "http_query": sys.argv[1],
    "grpc_bridge_query": sys.argv[2],
    "python_sdk_query": sys.argv[3],
    "typescript_sdk_query": sys.argv[4],
    "mcp_query": sys.argv[5],
}
setup_report_path = sys.argv[6]
report_path = sys.argv[7]

surfaces = {}
for name, path in surface_paths.items():
    payload = json.load(open(path, "r", encoding="utf-8"))
    rows = payload.get("rows") if isinstance(payload, dict) else None
    if not isinstance(rows, list):
        raise SystemExit(f"{name} payload does not contain rows array")

    first_chunk = None
    if rows and isinstance(rows[0], dict):
        first_chunk = rows[0].get("chunk_id")

    rows_canonical = json.dumps(rows, sort_keys=True, separators=(",", ":"))
    fingerprint = hashlib.sha256(rows_canonical.encode("utf-8")).hexdigest()

    surfaces[name] = {
        "kind": payload.get("kind"),
        "row_count": int(payload.get("row_count", len(rows))),
        "first_chunk_id": first_chunk,
        "rows_sha256": fingerprint,
    }

first_ids = [meta["first_chunk_id"] for meta in surfaces.values()]
row_counts = [meta["row_count"] for meta in surfaces.values()]

deterministic_first_chunk = len(set(first_ids)) == 1 and first_ids[0] is not None
deterministic_row_count = len(set(row_counts)) == 1 and row_counts[0] > 0
all_kind_query = all(meta["kind"] == "query" for meta in surfaces.values())

setup_report = json.load(open(setup_report_path, "r", encoding="utf-8"))

report = {
    "generated_unix_ms": int(time.time() * 1000),
    "setup_under_15_minutes": bool(setup_report.get("passes_under_15_minutes", False)),
    "deterministic_first_chunk": deterministic_first_chunk,
    "deterministic_row_count": deterministic_row_count,
    "all_kind_query": all_kind_query,
    "pass": deterministic_first_chunk and deterministic_row_count and all_kind_query and bool(setup_report.get("passes_under_15_minutes", False)),
    "surfaces": surfaces,
    "setup_report": {
        "elapsed_seconds": setup_report.get("elapsed_seconds"),
        "passes_under_15_minutes": setup_report.get("passes_under_15_minutes"),
    },
}

with open(report_path, "w", encoding="utf-8") as handle:
    json.dump(report, handle, indent=2)

print(f"deterministic_first_chunk={report['deterministic_first_chunk']}")
print(f"deterministic_row_count={report['deterministic_row_count']}")
print(f"all_kind_query={report['all_kind_query']}")
print(f"setup_under_15_minutes={report['setup_under_15_minutes']}")
print(f"pass={report['pass']}")

if not report["pass"]:
    raise SystemExit("contract suite assertions failed")
PY

echo "[s25-agent-contract-suite-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"
