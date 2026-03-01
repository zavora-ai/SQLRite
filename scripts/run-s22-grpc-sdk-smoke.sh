#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SERVER_BIN="${SERVER_BIN:-target/debug/sqlrite}"
CLIENT_BIN="${CLIENT_BIN:-target/debug/sqlrite-grpc-client}"
ADDR="${ADDR:-127.0.0.1:50091}"
DB_PATH="${DB_PATH:-project_plan/reports/s22_grpc_smoke.db}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s22_grpc_sdk_smoke.log}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" \
  /tmp/s22_health.json /tmp/s22_query.json /tmp/s22_sql.json

echo "[build] cargo build --bin sqlrite --bin sqlrite-grpc-client" | tee -a "$LOG_PATH"
cargo build --bin sqlrite --bin sqlrite-grpc-client >/dev/null

"$SERVER_BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s22_init.log 2>&1

echo "[start-grpc-server] addr=$ADDR" | tee -a "$LOG_PATH"
"$SERVER_BIN" grpc --db "$DB_PATH" --bind "$ADDR" >/tmp/sqlrite_s22_grpc_server.log 2>&1 &
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
  if "$CLIENT_BIN" --addr "$ADDR" health >/tmp/s22_health.json 2>/dev/null; then
    break
  fi
  sleep 0.1
done

if ! "$CLIENT_BIN" --addr "$ADDR" health >/tmp/s22_health.json 2>/dev/null; then
  echo "grpc server did not become ready" | tee -a "$LOG_PATH"
  tail -n 100 /tmp/sqlrite_s22_grpc_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[health]" | tee -a "$LOG_PATH"
jq '.' /tmp/s22_health.json | tee -a "$LOG_PATH" >/dev/null

echo "[query]" | tee -a "$LOG_PATH"
"$CLIENT_BIN" --addr "$ADDR" query --text "agent memory" --top-k 2 > /tmp/s22_query.json
jq '.' /tmp/s22_query.json | tee -a "$LOG_PATH" >/dev/null

echo "[sql]" | tee -a "$LOG_PATH"
"$CLIENT_BIN" --addr "$ADDR" sql --statement "SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;" > /tmp/s22_sql.json
jq '.' /tmp/s22_sql.json | tee -a "$LOG_PATH" >/dev/null

echo "[assertions]" | tee -a "$LOG_PATH"
jq -e '.status == "ok"' /tmp/s22_health.json >/dev/null
echo "grpc_health_ok=ok" | tee -a "$LOG_PATH"
jq -e '.kind == "query" and .row_count >= 1' /tmp/s22_query.json >/dev/null
echo "grpc_query_contract=ok" | tee -a "$LOG_PATH"
jq -e '.kind == "query" and .row_count >= 1' /tmp/s22_sql.json >/dev/null
echo "grpc_sql_contract=ok" | tee -a "$LOG_PATH"

echo "[s22-grpc-sdk-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
