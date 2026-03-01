#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8211}"
DB_PATH="${DB_PATH:-project_plan/reports/s21_openapi_grpc_smoke.db}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s21_openapi_grpc_smoke.log}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" /tmp/s21_openapi.json /tmp/s21_query.json /tmp/s21_grpc_query.json /tmp/s21_grpc_sql.json /tmp/s21_query_get.json /tmp/s21_grpc_sql_get.json

echo "[build] cargo build --bin sqlrite" | tee -a "$LOG_PATH"
cargo build --bin sqlrite >/dev/null

"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s21_init.log 2>&1

echo "[start-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve --db "$DB_PATH" --bind "$BIND_ADDR" >/tmp/sqlrite_s21_server.log 2>&1 &
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

for _ in $(seq 1 60); do
  if curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "server did not become ready" | tee -a "$LOG_PATH"
  tail -n 100 /tmp/sqlrite_s21_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[openapi]" | tee -a "$LOG_PATH"
curl -fsS "http://$BIND_ADDR/v1/openapi.json" > /tmp/s21_openapi.json
jq '.' /tmp/s21_openapi.json | tee -a "$LOG_PATH" >/dev/null

echo "[query-v1]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":2}' \
  "http://$BIND_ADDR/v1/query" > /tmp/s21_query.json
jq '.' /tmp/s21_query.json | tee -a "$LOG_PATH" >/dev/null

echo "[grpc-query]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":2}' \
  "http://$BIND_ADDR/grpc/sqlrite.v1.QueryService/Query" > /tmp/s21_grpc_query.json
jq '.' /tmp/s21_grpc_query.json | tee -a "$LOG_PATH" >/dev/null

echo "[grpc-sql]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  "http://$BIND_ADDR/grpc/sqlrite.v1.QueryService/Sql" > /tmp/s21_grpc_sql.json
jq '.' /tmp/s21_grpc_sql.json | tee -a "$LOG_PATH" >/dev/null

echo "[method-not-allowed]" | tee -a "$LOG_PATH"
curl -sS -X GET -H "accept: application/json" \
  "http://$BIND_ADDR/v1/query" > /tmp/s21_query_get.json
jq '.' /tmp/s21_query_get.json | tee -a "$LOG_PATH" >/dev/null
curl -sS -X GET -H "accept: application/json" \
  "http://$BIND_ADDR/grpc/sqlrite.v1.QueryService/Sql" > /tmp/s21_grpc_sql_get.json
jq '.' /tmp/s21_grpc_sql_get.json | tee -a "$LOG_PATH" >/dev/null

echo "[assertions]" | tee -a "$LOG_PATH"
jq -e '.openapi == "3.1.0"' /tmp/s21_openapi.json >/dev/null
echo "openapi_version=ok" | tee -a "$LOG_PATH"
jq -e '.paths["/v1/query"] != null' /tmp/s21_openapi.json >/dev/null
echo "openapi_has_v1_query=ok" | tee -a "$LOG_PATH"
jq -e '.paths["/grpc/sqlrite.v1.QueryService/Sql"] != null' /tmp/s21_openapi.json >/dev/null
echo "openapi_has_grpc_sql=ok" | tee -a "$LOG_PATH"
jq -e '.kind == "query" and .row_count >= 1' /tmp/s21_query.json >/dev/null
echo "v1_query_results=ok" | tee -a "$LOG_PATH"
jq -e '.kind == "query" and .row_count >= 1' /tmp/s21_grpc_query.json >/dev/null
echo "grpc_query_results=ok" | tee -a "$LOG_PATH"
jq -e '.kind == "query" and .row_count >= 1' /tmp/s21_grpc_sql.json >/dev/null
echo "grpc_sql_results=ok" | tee -a "$LOG_PATH"
grep -q 'method not allowed; use POST /v1/query' /tmp/s21_query_get.json
echo "v1_query_method_guard=ok" | tee -a "$LOG_PATH"
grep -q 'method not allowed; use POST /grpc/sqlrite.v1.QueryService/Sql' /tmp/s21_grpc_sql_get.json
echo "grpc_sql_method_guard=ok" | tee -a "$LOG_PATH"

echo "[s21-openapi-grpc-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
