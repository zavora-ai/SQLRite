#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
PORT="${PORT:-19139}"
DB_PATH="${DB_PATH:-project_plan/reports/s18_observability_smoke.db}"
BACKUP_DIR="${BACKUP_DIR:-project_plan/reports/s18_backups}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s18_observability_smoke.log}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s18-token}"
KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$BACKUP_DIR"

if command -v fuser >/dev/null 2>&1; then
  fuser -k "${PORT}/tcp" >/dev/null 2>&1 || true
fi

rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH"
rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"

"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s18_init.log 2>&1

"$BIN" serve \
  --db "$DB_PATH" \
  --bind "127.0.0.1:${PORT}" \
  --ha-role primary \
  --cluster-id s18-cluster \
  --node-id node-a \
  --advertise "127.0.0.1:${PORT}" \
  --peer 127.0.0.1:19140 \
  --peer 127.0.0.1:19141 \
  --sync-ack-quorum 2 \
  --failover manual \
  --backup-dir "$BACKUP_DIR" \
  --snapshot-interval-s 60 \
  --pitr-retention-s 3600 \
  --control-token "$CONTROL_TOKEN" \
  >/tmp/sqlrite_s18_server_stdout.log 2>&1 &
SERVER_PID=$!

cleanup() {
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  if [[ "$KEEP_ARTIFACTS" != "1" ]]; then
    rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
  fi
}
trap cleanup EXIT

for _ in $(seq 1 120); do
  if curl -sS "http://127.0.0.1:${PORT}/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

echo "[readyz]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/readyz" | tee -a "$LOG_PATH"

echo "\n[sql-success]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  "http://127.0.0.1:${PORT}/v1/sql" | tee -a "$LOG_PATH"

echo "\n[sql-failure]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT FROM bad_syntax"}' \
  "http://127.0.0.1:${PORT}/v1/sql" | tee -a "$LOG_PATH"

echo "\n[metrics-map]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/observability/metrics-map" | tee -a "$LOG_PATH"

echo "\n[recent-traces]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/traces/recent?limit=10" | tee -a "$LOG_PATH"

echo "\n[alert-templates]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/alerts/templates" | tee -a "$LOG_PATH"

echo "\n[alert-simulate]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"sql_error_rate":0.20,"sql_avg_latency_ms":75.0,"replication_lag_ms":1000}' \
  "http://127.0.0.1:${PORT}/control/v1/alerts/simulate" | tee -a "$LOG_PATH"

echo "\n[slo-report]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/slo/report" | tee -a "$LOG_PATH"

echo "\n[metrics]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/metrics" | \
  rg "sqlrite_(requests_|observability|alert_simulations|ha_|chunk_count|schema_version)" | \
  tee -a "$LOG_PATH"

echo "\n[s18-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
