#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
DB_PATH="${DB_PATH:-project_plan/reports/s25_agent_memory_setup.db}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8331}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s25_agent_memory_setup.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s25_agent_memory_setup.json}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" "$REPORT_PATH" \
  /tmp/s25_setup_python.json /tmp/s25_setup_ts.json

START_UNIX_MS=$(( $(date +%s) * 1000 ))

echo "[build] cargo build --bin sqlrite" | tee -a "$LOG_PATH"
cargo build --bin sqlrite >/dev/null

echo "[seed] $BIN init --db $DB_PATH --seed-demo" | tee -a "$LOG_PATH"
"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s25_setup_init.log 2>&1

echo "[start-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve --db "$DB_PATH" --bind "$BIND_ADDR" >/tmp/sqlrite_s25_setup_server.log 2>&1 &
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
  tail -n 80 /tmp/sqlrite_s25_setup_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[python-sdk-query]" | tee -a "$LOG_PATH"
PYTHONPATH="$ROOT_DIR/sdk/python" python3 - <<'PY' "$BIND_ADDR" > /tmp/s25_setup_python.json
import json
import sys
from sqlrite_sdk import SqlRiteClient
addr = sys.argv[1]
client = SqlRiteClient(f"http://{addr}")
payload = client.query(query_text="agent memory", top_k=2)
print(json.dumps(payload))
PY

python3 - <<'PY' /tmp/s25_setup_python.json
import json
import sys
payload = json.load(open(sys.argv[1], "r", encoding="utf-8"))
if payload.get("kind") != "query" or int(payload.get("row_count", 0)) < 1:
    raise SystemExit("python sdk query setup check failed")
PY

echo "[typescript-sdk-query]" | tee -a "$LOG_PATH"
npm --prefix "$ROOT_DIR/sdk/typescript" install >/tmp/s25_setup_npm_install.log 2>&1
npm --prefix "$ROOT_DIR/sdk/typescript" run build >/tmp/s25_setup_npm_build.log 2>&1

node - <<'NODE' "$BIND_ADDR" > /tmp/s25_setup_ts.json
import { SqlRiteClient } from './sdk/typescript/dist/index.js';
const addr = process.argv[2];
const client = new SqlRiteClient(`http://${addr}`);
const payload = await client.query({ query_text: 'agent memory', top_k: 2 });
console.log(JSON.stringify(payload));
NODE

python3 - <<'PY' /tmp/s25_setup_ts.json
import json
import sys
payload = json.load(open(sys.argv[1], "r", encoding="utf-8"))
if payload.get("kind") != "query" or int(payload.get("row_count", 0)) < 1:
    raise SystemExit("typescript sdk query setup check failed")
PY

END_UNIX_MS=$(( $(date +%s) * 1000 ))
ELAPSED_SECONDS=$(( (END_UNIX_MS - START_UNIX_MS) / 1000 ))
PASS_UNDER_15_MIN="false"
if [[ "$ELAPSED_SECONDS" -lt 900 ]]; then
  PASS_UNDER_15_MIN="true"
fi

python3 - <<'PY' "$REPORT_PATH" "$START_UNIX_MS" "$END_UNIX_MS" "$ELAPSED_SECONDS" "$PASS_UNDER_15_MIN"
import json
import sys
report = {
    "started_unix_ms": int(sys.argv[2]),
    "finished_unix_ms": int(sys.argv[3]),
    "elapsed_seconds": int(sys.argv[4]),
    "passes_under_15_minutes": sys.argv[5].lower() == "true",
    "setup_target_seconds": 900,
    "steps": [
        "build_sqlrite",
        "seed_demo_db",
        "start_server",
        "python_sdk_query",
        "typescript_sdk_query",
    ],
}
with open(sys.argv[1], "w", encoding="utf-8") as handle:
    json.dump(report, handle, indent=2)
PY

echo "elapsed_seconds=$ELAPSED_SECONDS" | tee -a "$LOG_PATH"
echo "passes_under_15_minutes=$PASS_UNDER_15_MIN" | tee -a "$LOG_PATH"
echo "[s25-agent-memory-setup-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"

if [[ "$PASS_UNDER_15_MIN" != "true" ]]; then
  echo "setup exceeded 15 minute target" | tee -a "$LOG_PATH"
  exit 1
fi
