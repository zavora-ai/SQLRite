#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
PORT="${PORT:-19129}"
DB_PATH="${DB_PATH:-project_plan/reports/s17_server_recovery_smoke.db}"
BACKUP_DIR="${BACKUP_DIR:-project_plan/reports/s17_backups}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s17_backup_pitr_smoke.log}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s17-token}"
KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$BACKUP_DIR"

if command -v fuser >/dev/null 2>&1; then
  fuser -k "${PORT}/tcp" >/dev/null 2>&1 || true
fi

rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH"
rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"

"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/sqlrite_s17_init.log 2>&1

"$BIN" serve \
  --db "$DB_PATH" \
  --bind "127.0.0.1:${PORT}" \
  --ha-role primary \
  --cluster-id s17-cluster \
  --node-id node-a \
  --advertise "127.0.0.1:${PORT}" \
  --peer 127.0.0.1:19130 \
  --peer 127.0.0.1:19131 \
  --sync-ack-quorum 2 \
  --failover manual \
  --backup-dir "$BACKUP_DIR" \
  --snapshot-interval-s 60 \
  --pitr-retention-s 3600 \
  --control-token "$CONTROL_TOKEN" \
  >/tmp/sqlrite_s17_server_stdout.log 2>&1 &
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

echo "\n[snapshot-via-control-plane]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"note":"control_plane_snapshot"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/snapshot" | tee -a "$LOG_PATH"

echo "\n[snapshots-via-control-plane]" | tee -a "$LOG_PATH"
curl -sS "http://127.0.0.1:${PORT}/control/v1/recovery/snapshots?limit=10" | tee -a "$LOG_PATH"

echo "\n[verify-restore-via-control-plane]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"keep_artifact":false,"note":"drill_verify_restore"}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/verify-restore" | tee -a "$LOG_PATH"

echo "\n[snapshot-via-cli]" | tee -a "$LOG_PATH"
"$BIN" backup snapshot \
  --source "$DB_PATH" \
  --backup-dir "$BACKUP_DIR" \
  --note "cli_snapshot" \
  --json | tee -a "$LOG_PATH"

echo "\n[list-via-cli]" | tee -a "$LOG_PATH"
"$BIN" backup list --backup-dir "$BACKUP_DIR" --json | tee -a "$LOG_PATH"

TARGET_MS=$(( $(date +%s) * 1000 + 999 ))
RESTORE_DEST="${DB_PATH}.restored.db"
rm -f "$RESTORE_DEST" "$RESTORE_DEST-wal" "$RESTORE_DEST-shm"

echo "\n[pitr-restore-via-cli]" | tee -a "$LOG_PATH"
"$BIN" backup pitr-restore \
  --backup-dir "$BACKUP_DIR" \
  --target-unix-ms "$TARGET_MS" \
  --dest "$RESTORE_DEST" \
  --verify \
  --json | tee -a "$LOG_PATH"

echo "\n[verify-restored-db-via-cli]" | tee -a "$LOG_PATH"
"$BIN" backup verify --path "$RESTORE_DEST" | tee -a "$LOG_PATH"

echo "\n[prune-via-control-plane]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: ${CONTROL_TOKEN}" \
  -d '{"retention_seconds":0}' \
  "http://127.0.0.1:${PORT}/control/v1/recovery/prune-snapshots" | tee -a "$LOG_PATH"

echo "\n[s17-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
