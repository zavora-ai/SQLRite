#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
PORT="${PORT:-19149}"
DB_PATH="${DB_PATH:-project_plan/reports/s19_dr_gameday.db}"
BACKUP_DIR="${BACKUP_DIR:-project_plan/reports/s19_backups}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s19_dr_gameday.log}"
SUMMARY_PATH="${SUMMARY_PATH:-project_plan/reports/s19_soak_slo_summary.json}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s19-token}"
SOAK_REQUESTS="${SOAK_REQUESTS:-200}"
KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$BACKUP_DIR"

if command -v fuser >/dev/null 2>&1; then
  fuser -k "${PORT}/tcp" >/dev/null 2>&1 || true
fi

rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" "$SUMMARY_PATH"
rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"

"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s19_init.log 2>&1

"$BIN" serve \
  --db "$DB_PATH" \
  --bind "127.0.0.1:${PORT}" \
  --ha-role replica \
  --cluster-id s19-cluster \
  --node-id node-b \
  --advertise "127.0.0.1:${PORT}" \
  --peer 127.0.0.1:19150 \
  --peer 127.0.0.1:19151 \
  --sync-ack-quorum 2 \
  --failover automatic \
  --backup-dir "$BACKUP_DIR" \
  --snapshot-interval-s 60 \
  --pitr-retention-s 3600 \
  --control-token "$CONTROL_TOKEN" \
  >/tmp/sqlrite_s19_server_stdout.log 2>&1 &
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

echo "[readyz-initial]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/readyz" | tee -a "$LOG_PATH"

echo "\n[heartbeat-and-auto-failover]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"term":1,"leader_id":"node-a","commit_index":0,"leader_last_log_index":0,"replication_lag_ms":5}' \
  "http://127.0.0.1:${PORT}/control/v1/election/heartbeat" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"simulate_elapsed_ms":5000,"reason":"s19_soak_promote"}' \
  "http://127.0.0.1:${PORT}/control/v1/failover/auto-check" | tee -a "$LOG_PATH"

echo "\n[snapshot-and-verify-restore]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"note":"s19_gameday_snapshot"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/snapshot" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"keep_artifact":false,"note":"s19_verify_restore"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/verify-restore" | tee -a "$LOG_PATH"

echo "\n[chaos-partition]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"partition_subset","note":"s19_partition"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/inject" | tee -a "$LOG_PATH"
PARTITION_CODE=$(curl -sS -o /tmp/s19_partition.json -w "%{http_code}" -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"term":2,"leader_id":"node-a","commit_index":1,"leader_last_log_index":1}' \
  "http://127.0.0.1:${PORT}/control/v1/election/heartbeat")
echo "heartbeat_during_partition_status=${PARTITION_CODE}" | tee -a "$LOG_PATH"
cat /tmp/s19_partition.json | tee -a "$LOG_PATH"
curl -sS -X POST -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  "http://127.0.0.1:${PORT}/control/v1/chaos/clear" | tee -a "$LOG_PATH"

echo "\n[chaos-disk-full]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"disk_full","note":"s19_disk"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/inject" | tee -a "$LOG_PATH"
DISK_CODE=$(curl -sS -o /tmp/s19_disk.json -w "%{http_code}" -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"operation":"ingest_chunk","payload":{"chunk_id":"s19"}}' \
  "http://127.0.0.1:${PORT}/control/v1/replication/append")
echo "append_during_disk_full_status=${DISK_CODE}" | tee -a "$LOG_PATH"
cat /tmp/s19_disk.json | tee -a "$LOG_PATH"
curl -sS -X POST -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  "http://127.0.0.1:${PORT}/control/v1/chaos/clear" | tee -a "$LOG_PATH"

echo "\n[chaos-node-crash]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"scenario":"node_crash","note":"s19_node_crash"}' \
  "http://127.0.0.1:${PORT}/control/v1/chaos/inject" | tee -a "$LOG_PATH"
CRASH_CODE=$(curl -sS -o /tmp/s19_crash.json -w "%{http_code}" \
  "http://127.0.0.1:${PORT}/readyz")
echo "readyz_during_node_crash_status=${CRASH_CODE}" | tee -a "$LOG_PATH"
cat /tmp/s19_crash.json | tee -a "$LOG_PATH"
curl -sS -X POST -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  "http://127.0.0.1:${PORT}/control/v1/chaos/clear" | tee -a "$LOG_PATH"

echo "\n[reset-observability-window]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  "http://127.0.0.1:${PORT}/control/v1/observability/reset" | tee -a "$LOG_PATH"

echo "\n[soak-loop]" | tee -a "$LOG_PATH"
SUCCESS=0
FAIL=0
for _ in $(seq 1 "$SOAK_REQUESTS"); do
  CODE=$(curl -sS -o /dev/null -w "%{http_code}" "http://127.0.0.1:${PORT}/readyz")
  if [[ "$CODE" -ge 500 ]]; then
    FAIL=$((FAIL + 1))
  else
    SUCCESS=$((SUCCESS + 1))
  fi
done
echo "soak_total=${SOAK_REQUESTS}" | tee -a "$LOG_PATH"
echo "soak_success=${SUCCESS}" | tee -a "$LOG_PATH"
echo "soak_fail=${FAIL}" | tee -a "$LOG_PATH"

AVAILABILITY=$(awk -v s="$SUCCESS" -v t="$SOAK_REQUESTS" 'BEGIN { printf "%.4f", (s/t)*100.0 }')
AVAILABILITY_PASS=$(awk -v a="$AVAILABILITY" 'BEGIN { print (a>=99.95) ? "true" : "false" }')

RPO_MS=$(curl -sS "http://127.0.0.1:${PORT}/metrics" | sed -n 's/^sqlrite_ha_replication_lag_ms //p' | head -n1)
RPO_MS="${RPO_MS:-0}"
RPO_SECONDS=$(awk -v ms="$RPO_MS" 'BEGIN { printf "%.4f", ms/1000.0 }')
RPO_PASS=$(awk -v sec="$RPO_SECONDS" 'BEGIN { print (sec<=60.0) ? "true" : "false" }')

echo "\n[slo-report-endpoint]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/slo/report" | tee -a "$LOG_PATH"

cat > "$SUMMARY_PATH" <<EOF
{
  "soak_total_requests": $SOAK_REQUESTS,
  "soak_success_requests": $SUCCESS,
  "soak_server_error_requests": $FAIL,
  "availability_percent": $AVAILABILITY,
  "availability_target_percent": 99.95,
  "availability_pass": $AVAILABILITY_PASS,
  "observed_rpo_seconds": $RPO_SECONDS,
  "rpo_target_seconds": 60.0,
  "rpo_pass": $RPO_PASS,
  "chaos_validation": {
    "partition_status_code": $PARTITION_CODE,
    "disk_full_status_code": $DISK_CODE,
    "node_crash_status_code": $CRASH_CODE
  }
}
EOF

echo "\n[s19-summary]" | tee -a "$LOG_PATH"
cat "$SUMMARY_PATH" | tee -a "$LOG_PATH"

echo "\n[s19-gameday-complete] log=${LOG_PATH} summary=${SUMMARY_PATH}" | tee -a "$LOG_PATH"
