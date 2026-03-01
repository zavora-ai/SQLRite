#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
PORT="${PORT:-19119}"
DB_PATH="${DB_PATH:-project_plan/reports/s16_server_smoke.db}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s16_failover_chaos_smoke.log}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s16-token}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")"

if command -v fuser >/dev/null 2>&1; then
  fuser -k "${PORT}/tcp" >/dev/null 2>&1 || true
else
  EXISTING_PIDS="$(lsof -t -iTCP:${PORT} -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$EXISTING_PIDS" ]]; then
    # smoke harness assumes a dedicated local test port.
    kill $EXISTING_PIDS >/dev/null 2>&1 || true
  fi
fi

rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH"

"$BIN" serve \
  --db "$DB_PATH" \
  --bind "127.0.0.1:${PORT}" \
  --ha-role replica \
  --cluster-id s16-cluster \
  --node-id node-b \
  --advertise "127.0.0.1:${PORT}" \
  --peer 127.0.0.1:19118 \
  --peer 127.0.0.1:19120 \
  --sync-ack-quorum 2 \
  --failover automatic \
  --control-token "$CONTROL_TOKEN" \
  >/tmp/sqlrite_s16_server_stdout.log 2>&1 &
SERVER_PID=$!

cleanup() {
  kill "$SERVER_PID" >/dev/null 2>&1 || true
  rm -f /tmp/s16_partition_resp.json /tmp/s16_disk_full_resp.json
  if [[ "$KEEP_DB" != "1" ]]; then
    rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
  fi
}
trap cleanup EXIT

for _ in $(seq 1 120); do
  if ! kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    echo "server exited before readiness; see /tmp/sqlrite_s16_server_stdout.log" >&2
    exit 1
  fi
  if curl -sS "http://127.0.0.1:${PORT}/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -sS "http://127.0.0.1:${PORT}/readyz" >/dev/null 2>&1; then
  echo "server did not become ready on port ${PORT}" >&2
  exit 1
fi

echo "[readyz-initial]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/readyz" | tee -a "$LOG_PATH"

echo "\n[heartbeat-from-primary]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"term":1,"leader_id":"node-a","commit_index":0,"leader_last_log_index":0,"replication_lag_ms":5}' \
  "http://127.0.0.1:${PORT}/control/v1/election/heartbeat" | tee -a "$LOG_PATH"

echo "\n[auto-failover-check]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"simulate_elapsed_ms":5000,"reason":"leader_timeout_test"}' \
  "http://127.0.0.1:${PORT}/control/v1/failover/auto-check" | tee -a "$LOG_PATH"

echo "\n[chaos-inject-partition]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"partition_subset","note":"block-heartbeats"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/inject" | tee -a "$LOG_PATH"

echo "\n[heartbeat-during-partition]" | tee -a "$LOG_PATH"
HTTP_CODE=$(curl -sS -o /tmp/s16_partition_resp.json -w "%{http_code}" -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"term":2,"leader_id":"node-a","commit_index":1,"leader_last_log_index":1}' \
  "http://127.0.0.1:${PORT}/control/v1/election/heartbeat")
echo "status=${HTTP_CODE}" | tee -a "$LOG_PATH"
cat /tmp/s16_partition_resp.json | tee -a "$LOG_PATH"

echo "\n[chaos-clear-partition]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"partition_subset"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/clear" | tee -a "$LOG_PATH"

echo "\n[chaos-inject-disk-full]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"disk_full"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/inject" | tee -a "$LOG_PATH"

echo "\n[replication-append-disk-full]" | tee -a "$LOG_PATH"
HTTP_CODE=$(curl -sS -o /tmp/s16_disk_full_resp.json -w "%{http_code}" -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"operation":"ingest_chunk","payload":{"chunk_id":"c1"}}' \
  "http://127.0.0.1:${PORT}/control/v1/replication/append")
echo "status=${HTTP_CODE}" | tee -a "$LOG_PATH"
cat /tmp/s16_disk_full_resp.json | tee -a "$LOG_PATH"

echo "\n[chaos-clear-all]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  "http://127.0.0.1:${PORT}/control/v1/chaos/clear" | tee -a "$LOG_PATH"

echo "\n[recovery-start]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"note":"restore_drill"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/start" | tee -a "$LOG_PATH"
sleep 0.06

echo "\n[recovery-mark-restored]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"backup_artifact":"/tmp/s16.backup","note":"restore_complete"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/mark-restored" | tee -a "$LOG_PATH"

echo "\n[resilience]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/resilience" | tee -a "$LOG_PATH"

echo "\n[metrics]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/metrics" | \
  rg "sqlrite_ha_(failover|restore|chaos)|sqlrite_ha_(role|term|commit_index|last_log_index|last_log_term|replication_lag_ms|replication_log_entries|enabled|failover_in_progress)" | \
  tee -a "$LOG_PATH"

echo "\n[chaos-status-final]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/chaos/status" | tee -a "$LOG_PATH"

echo "\n[s16-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
