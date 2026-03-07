#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
SECURITY_BIN="${SECURITY_BIN:-target/debug/sqlrite-security}"
DB_PATH="${DB_PATH:-project_plan/reports/s28_security_audit.db}"
REGISTRY_PATH="${REGISTRY_PATH:-project_plan/reports/s28_rotation_keys.json}"
AUDIT_PATH="${AUDIT_PATH:-project_plan/reports/s28_security_audit.jsonl}"
POLICY_PATH="${POLICY_PATH:-project_plan/reports/s28_rbac_policy.json}"
SERVER_EXPORT_PATH="${SERVER_EXPORT_PATH:-project_plan/reports/s28_audit_export_server.jsonl}"
CLI_EXPORT_PATH="${CLI_EXPORT_PATH:-project_plan/reports/s28_audit_export.jsonl}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s28_security_audit_hardening.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s28_security_audit_report.json}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8348}"
CONTROL_TOKEN="${CONTROL_TOKEN:-s28-control}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$REGISTRY_PATH" "$AUDIT_PATH" "$POLICY_PATH" \
  "$SERVER_EXPORT_PATH" "$CLI_EXPORT_PATH" "$LOG_PATH" "$REPORT_PATH" \
  /tmp/s28_rerank_hook.json /tmp/s28_server_audit_export.json /tmp/s28_verify_before.json /tmp/s28_rotate_report.json /tmp/s28_verify_after.json /tmp/s28_server.log

echo "[build]" | tee -a "$LOG_PATH"
cargo build --bin sqlrite --bin sqlrite-security >/dev/null

echo "[init-policy]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" init-policy --path "$POLICY_PATH" | tee -a "$LOG_PATH"

echo "[seed-rotation-demo]" | tee -a "$LOG_PATH"
cargo run --quiet --example security_rotation_workflow -- "$DB_PATH" "$REGISTRY_PATH" "$AUDIT_PATH" | tee -a "$LOG_PATH"

echo "[start-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve \
  --db "$DB_PATH" \
  --bind "$BIND_ADDR" \
  --secure-defaults \
  --authz-policy "$POLICY_PATH" \
  --audit-log "$AUDIT_PATH" \
  --control-token "$CONTROL_TOKEN" \
  >/tmp/s28_server.log 2>&1 &
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

for _ in $(seq 1 120); do
  if curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "server did not become ready" | tee -a "$LOG_PATH"
  tail -n 120 /tmp/s28_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[query-and-rerank]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"rotation workflow","candidate_count":5}' \
  "http://$BIND_ADDR/v1/rerank-hook" > /tmp/s28_rerank_hook.json
jq '.' /tmp/s28_rerank_hook.json | tee -a "$LOG_PATH" >/dev/null

echo "[server-audit-export]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $CONTROL_TOKEN" \
  -d "{\"tenant_id\":\"demo\",\"output_path\":\"$SERVER_EXPORT_PATH\",\"format\":\"jsonl\"}" \
  "http://$BIND_ADDR/control/v1/security/audit/export" > /tmp/s28_server_audit_export.json
jq '.' /tmp/s28_server_audit_export.json | tee -a "$LOG_PATH" >/dev/null

echo "[cli-audit-export]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" export-audit \
  --input "$AUDIT_PATH" \
  --output "$CLI_EXPORT_PATH" \
  --format jsonl \
  --tenant demo | tee -a "$LOG_PATH"

echo "[verify-before]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" verify-key \
  --db "$DB_PATH" \
  --registry "$REGISTRY_PATH" \
  --tenant demo \
  --field secret_payload \
  --key-id k2 > /tmp/s28_verify_before.json
jq '.' /tmp/s28_verify_before.json | tee -a "$LOG_PATH" >/dev/null

echo "[rotate-key]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" rotate-key \
  --db "$DB_PATH" \
  --registry "$REGISTRY_PATH" \
  --tenant demo \
  --field secret_payload \
  --new-key-id k2 \
  --json > /tmp/s28_rotate_report.json
jq '.' /tmp/s28_rotate_report.json | tee -a "$LOG_PATH" >/dev/null

echo "[verify-after]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" verify-key \
  --db "$DB_PATH" \
  --registry "$REGISTRY_PATH" \
  --tenant demo \
  --field secret_payload \
  --key-id k2 > /tmp/s28_verify_after.json
jq '.' /tmp/s28_verify_after.json | tee -a "$LOG_PATH" >/dev/null

echo "[assertions]" | tee -a "$LOG_PATH"
python3 - <<'PY' \
  /tmp/s28_rerank_hook.json \
  /tmp/s28_server_audit_export.json \
  /tmp/s28_verify_before.json \
  /tmp/s28_rotate_report.json \
  /tmp/s28_verify_after.json \
  "$SERVER_EXPORT_PATH" \
  "$CLI_EXPORT_PATH" \
  "$REPORT_PATH" | tee -a "$LOG_PATH"
import json
import pathlib
import sys
import time

rerank = json.load(open(sys.argv[1], "r", encoding="utf-8"))
server_export = json.load(open(sys.argv[2], "r", encoding="utf-8"))
verify_before = json.load(open(sys.argv[3], "r", encoding="utf-8"))
rotate_report = json.load(open(sys.argv[4], "r", encoding="utf-8"))
verify_after = json.load(open(sys.argv[5], "r", encoding="utf-8"))
server_export_path = pathlib.Path(sys.argv[6])
cli_export_path = pathlib.Path(sys.argv[7])
report_path = pathlib.Path(sys.argv[8])

report = {
    "generated_unix_ms": int(time.time() * 1000),
    "rerank_hook_ok": rerank.get("kind") == "rerank_hook" and int(rerank.get("row_count", 0)) >= 1,
    "server_audit_export_ok": int(server_export.get("matched_events", 0)) >= 1 and server_export_path.exists(),
    "cli_audit_export_ok": cli_export_path.exists() and cli_export_path.read_text(encoding="utf-8").strip() != "",
    "verify_before_detects_stale_keys": not bool(verify_before.get("verified_all_target_key", True)),
    "rotation_updated_chunks": int(rotate_report.get("rotated_chunks", 0)),
    "rotation_verified": bool(rotate_report.get("verified_all_target_key", False)),
    "verify_after_all_target_key": bool(verify_after.get("verified_all_target_key", False)),
    "stale_key_ids_after": verify_after.get("stale_key_ids", []),
}
report["pass"] = all([
    report["rerank_hook_ok"],
    report["server_audit_export_ok"],
    report["cli_audit_export_ok"],
    report["verify_before_detects_stale_keys"],
    report["rotation_updated_chunks"] >= 1,
    report["rotation_verified"],
    report["verify_after_all_target_key"],
    report["stale_key_ids_after"] == [],
])
report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(f"rerank_hook_ok={report['rerank_hook_ok']}")
print(f"server_audit_export_ok={report['server_audit_export_ok']}")
print(f"cli_audit_export_ok={report['cli_audit_export_ok']}")
print(f"rotation_updated_chunks={report['rotation_updated_chunks']}")
print(f"verify_after_all_target_key={report['verify_after_all_target_key']}")
print(f"pass={report['pass']}")
if not report["pass"]:
    raise SystemExit("s28 security audit hardening assertions failed")
PY

echo "[s28-security-audit-hardening-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"
