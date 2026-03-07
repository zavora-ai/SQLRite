#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
SECURITY_BIN="${SECURITY_BIN:-target/debug/sqlrite-security}"
DB_PATH="${DB_PATH:-project_plan/reports/s27_security_rbac.db}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8347}"
POLICY_PATH="${POLICY_PATH:-project_plan/reports/s27_rbac_policy.json}"
AUDIT_PATH="${AUDIT_PATH:-project_plan/reports/s27_security_audit.jsonl}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s27_security_rbac_smoke.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s27_security_rbac_report.json}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$POLICY_PATH" "$AUDIT_PATH" "$LOG_PATH" "$REPORT_PATH" \
  /tmp/s27_security_summary.json /tmp/s27_query_denied.json /tmp/s27_query_ok.json \
  /tmp/s27_query_mismatch.json /tmp/s27_sql_reader.json /tmp/s27_sql_admin.json /tmp/s27_server.log

echo "[build]" | tee -a "$LOG_PATH"
cargo build --bin sqlrite --bin sqlrite-security >/dev/null

echo "[seed-db]" | tee -a "$LOG_PATH"
"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/s27_init.log 2>&1

echo "[init-policy]" | tee -a "$LOG_PATH"
"$SECURITY_BIN" init-policy --path "$POLICY_PATH" | tee -a "$LOG_PATH"

echo "[start-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve \
  --db "$DB_PATH" \
  --bind "$BIND_ADDR" \
  --secure-defaults \
  --authz-policy "$POLICY_PATH" \
  --audit-log "$AUDIT_PATH" \
  >/tmp/s27_server.log 2>&1 &
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
  tail -n 120 /tmp/s27_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[security-summary]" | tee -a "$LOG_PATH"
curl -fsS "http://$BIND_ADDR/control/v1/security" > /tmp/s27_security_summary.json
jq '.' /tmp/s27_security_summary.json | tee -a "$LOG_PATH" >/dev/null

echo "[query-without-headers]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent","top_k":1}' \
  "http://$BIND_ADDR/v1/query" > /tmp/s27_query_denied.json
jq '.' /tmp/s27_query_denied.json | tee -a "$LOG_PATH" >/dev/null

echo "[query-reader-ok]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent","top_k":2}' \
  "http://$BIND_ADDR/v1/query" > /tmp/s27_query_ok.json
jq '.' /tmp/s27_query_ok.json | tee -a "$LOG_PATH" >/dev/null

echo "[query-reader-tenant-mismatch]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent","top_k":2,"metadata_filters":{"tenant":"beta"}}' \
  "http://$BIND_ADDR/v1/query" > /tmp/s27_query_mismatch.json
jq '.' /tmp/s27_query_mismatch.json | tee -a "$LOG_PATH" >/dev/null

echo "[sql-reader-denied]" | tee -a "$LOG_PATH"
curl -sS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}' \
  "http://$BIND_ADDR/v1/sql" > /tmp/s27_sql_reader.json
jq '.' /tmp/s27_sql_reader.json | tee -a "$LOG_PATH" >/dev/null

echo "[sql-admin-ok]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: admin-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: admin" \
  -d '{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}' \
  "http://$BIND_ADDR/v1/sql" > /tmp/s27_sql_admin.json
jq '.' /tmp/s27_sql_admin.json | tee -a "$LOG_PATH" >/dev/null

echo "[assertions]" | tee -a "$LOG_PATH"
python3 - <<'PY' \
  /tmp/s27_security_summary.json \
  /tmp/s27_query_denied.json \
  /tmp/s27_query_ok.json \
  /tmp/s27_query_mismatch.json \
  /tmp/s27_sql_reader.json \
  /tmp/s27_sql_admin.json \
  "$AUDIT_PATH" \
  "$REPORT_PATH" | tee -a "$LOG_PATH"
import json
import pathlib
import sys
import time

summary = json.load(open(sys.argv[1], "r", encoding="utf-8"))
query_denied = json.load(open(sys.argv[2], "r", encoding="utf-8"))
query_ok = json.load(open(sys.argv[3], "r", encoding="utf-8"))
query_mismatch = json.load(open(sys.argv[4], "r", encoding="utf-8"))
sql_reader = json.load(open(sys.argv[5], "r", encoding="utf-8"))
sql_admin = json.load(open(sys.argv[6], "r", encoding="utf-8"))
audit_path = pathlib.Path(sys.argv[7])
report_path = pathlib.Path(sys.argv[8])
audit_lines = audit_path.read_text(encoding="utf-8").strip().splitlines() if audit_path.exists() else []

report = {
    "generated_unix_ms": int(time.time() * 1000),
    "security_enabled": bool(summary.get("enabled")),
    "require_auth_context": bool(summary.get("require_auth_context")),
    "rbac_role_count": len(summary.get("rbac_roles", [])),
    "query_without_headers_denied": "missing auth context" in query_denied.get("error", ""),
    "query_reader_ok": query_ok.get("kind") == "query" and int(query_ok.get("row_count", 0)) >= 1,
    "query_tenant_mismatch_denied": "tenant filter mismatch" in query_mismatch.get("error", ""),
    "sql_reader_denied": "authorization denied" in sql_reader.get("error", ""),
    "sql_admin_ok": int(sql_admin.get("row_count", 0)) >= 1,
    "audit_log_exists": audit_path.exists(),
    "audit_allowed_entries": sum('"allowed":true' in line for line in audit_lines),
    "audit_denied_entries": sum('"allowed":false' in line for line in audit_lines),
}
report["pass"] = all([
    report["security_enabled"],
    report["require_auth_context"],
    report["rbac_role_count"] >= 4,
    report["query_without_headers_denied"],
    report["query_reader_ok"],
    report["query_tenant_mismatch_denied"],
    report["sql_reader_denied"],
    report["sql_admin_ok"],
    report["audit_log_exists"],
    report["audit_allowed_entries"] >= 1,
    report["audit_denied_entries"] >= 1,
])

report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(f"security_enabled={report['security_enabled']}")
print(f"query_without_headers_denied={report['query_without_headers_denied']}")
print(f"query_reader_ok={report['query_reader_ok']}")
print(f"sql_reader_denied={report['sql_reader_denied']}")
print(f"sql_admin_ok={report['sql_admin_ok']}")
print(f"audit_allowed_entries={report['audit_allowed_entries']}")
print(f"audit_denied_entries={report['audit_denied_entries']}")
print(f"pass={report['pass']}")

if not report["pass"]:
    raise SystemExit("s27 security smoke assertions failed")
PY

echo "[s27-security-rbac-smoke-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"
