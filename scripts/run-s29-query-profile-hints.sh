#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
GRPC_CLIENT_BIN="${GRPC_CLIENT_BIN:-target/debug/sqlrite-grpc-client}"
DB_PATH="${DB_PATH:-project_plan/reports/s29_query_profiles.db}"
HTTP_BIND_ADDR="${HTTP_BIND_ADDR:-127.0.0.1:8349}"
GRPC_BIND_ADDR="${GRPC_BIND_ADDR:-127.0.0.1:50083}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s29_query_profile_hints.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s29_query_profile_report.json}"
LATENCY_BENCH_PATH="${LATENCY_BENCH_PATH:-project_plan/reports/s29_benchmark_latency_profile.json}"
RECALL_BENCH_PATH="${RECALL_BENCH_PATH:-project_plan/reports/s29_benchmark_recall_profile.json}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")"
rm -f \
  "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" \
  "$LOG_PATH" "$REPORT_PATH" "$LATENCY_BENCH_PATH" "$RECALL_BENCH_PATH" \
  /tmp/s29_cli_latency.txt /tmp/s29_cli_recall.txt \
  /tmp/s29_http_latency.json /tmp/s29_http_recall.json /tmp/s29_grpc_recall.json \
  /tmp/s29_server.log /tmp/s29_grpc.log

echo "[build]" | tee -a "$LOG_PATH"
cargo build --bin sqlrite --bin sqlrite-grpc-client >/dev/null

echo "[init-db]" | tee -a "$LOG_PATH"
"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/s29_init.log 2>&1

echo "[cli-latency]" | tee -a "$LOG_PATH"
"$BIN" query \
  --db "$DB_PATH" \
  --text "agents local memory" \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile latency > /tmp/s29_cli_latency.txt
cat /tmp/s29_cli_latency.txt >> "$LOG_PATH"

echo "[cli-recall]" | tee -a "$LOG_PATH"
"$BIN" query \
  --db "$DB_PATH" \
  --text "agents local memory" \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile recall > /tmp/s29_cli_recall.txt
cat /tmp/s29_cli_recall.txt >> "$LOG_PATH"

echo "[start-http-server] bind=$HTTP_BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve --db "$DB_PATH" --bind "$HTTP_BIND_ADDR" >/tmp/s29_server.log 2>&1 &
HTTP_PID=$!

echo "[start-grpc-server] bind=$GRPC_BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" grpc --db "$DB_PATH" --bind "$GRPC_BIND_ADDR" >/tmp/s29_grpc.log 2>&1 &
GRPC_PID=$!

cleanup() {
  for pid in "$HTTP_PID" "$GRPC_PID"; do
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
    fi
  done
  if [[ "$KEEP_DB" != "1" ]]; then
    rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
  fi
}
trap cleanup EXIT

for _ in $(seq 1 120); do
  if curl -fsS "http://$HTTP_BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$HTTP_BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "http server did not become ready" | tee -a "$LOG_PATH"
  tail -n 120 /tmp/s29_server.log >> "$LOG_PATH" || true
  exit 1
fi

for _ in $(seq 1 120); do
  if "$GRPC_CLIENT_BIN" --addr "$GRPC_BIND_ADDR" health >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! "$GRPC_CLIENT_BIN" --addr "$GRPC_BIND_ADDR" health >/dev/null 2>&1; then
  echo "grpc server did not become ready" | tee -a "$LOG_PATH"
  tail -n 120 /tmp/s29_grpc.log >> "$LOG_PATH" || true
  exit 1
fi

echo "[http-query-latency]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agents local memory","top_k":5,"candidate_limit":1000,"query_profile":"latency"}' \
  "http://$HTTP_BIND_ADDR/v1/query" > /tmp/s29_http_latency.json

echo "[http-query-recall]" | tee -a "$LOG_PATH"
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agents local memory","top_k":5,"candidate_limit":1000,"query_profile":"recall"}' \
  "http://$HTTP_BIND_ADDR/v1/query" > /tmp/s29_http_recall.json

echo "[grpc-query-recall]" | tee -a "$LOG_PATH"
"$GRPC_CLIENT_BIN" \
  --addr "$GRPC_BIND_ADDR" \
  query \
  --text "agents local memory" \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile recall > /tmp/s29_grpc_recall.json

echo "[benchmark-latency]" | tee -a "$LOG_PATH"
"$BIN" benchmark \
  --corpus 5000 \
  --queries 200 \
  --warmup 20 \
  --embedding-dim 16 \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile latency \
  --alpha 0.65 \
  --fusion weighted \
  --index-mode hnsw_baseline \
  --output "$LATENCY_BENCH_PATH" | tee -a "$LOG_PATH"

echo "[benchmark-recall]" | tee -a "$LOG_PATH"
"$BIN" benchmark \
  --corpus 5000 \
  --queries 200 \
  --warmup 20 \
  --embedding-dim 16 \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile recall \
  --alpha 0.65 \
  --fusion weighted \
  --index-mode hnsw_baseline \
  --output "$RECALL_BENCH_PATH" | tee -a "$LOG_PATH"

echo "[assertions]" | tee -a "$LOG_PATH"
python3 - <<'PY' \
  /tmp/s29_cli_latency.txt \
  /tmp/s29_cli_recall.txt \
  /tmp/s29_http_latency.json \
  /tmp/s29_http_recall.json \
  /tmp/s29_grpc_recall.json \
  "$LATENCY_BENCH_PATH" \
  "$RECALL_BENCH_PATH" \
  "$REPORT_PATH" | tee -a "$LOG_PATH"
import json
import pathlib
import sys
import time

cli_latency = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
cli_recall = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
http_latency = json.load(open(sys.argv[3], "r", encoding="utf-8"))
http_recall = json.load(open(sys.argv[4], "r", encoding="utf-8"))
grpc_recall = json.load(open(sys.argv[5], "r", encoding="utf-8"))
latency_bench = json.load(open(sys.argv[6], "r", encoding="utf-8"))
recall_bench = json.load(open(sys.argv[7], "r", encoding="utf-8"))
report_path = pathlib.Path(sys.argv[8])

report = {
    "generated_unix_ms": int(time.time() * 1000),
    "cli_latency_ok": "query_profile=latency resolved_candidate_limit=40" in cli_latency,
    "cli_recall_ok": "query_profile=recall resolved_candidate_limit=1000" in cli_recall,
    "http_latency_ok": http_latency.get("kind") == "query" and int(http_latency.get("row_count", 0)) >= 1,
    "http_recall_ok": http_recall.get("kind") == "query" and int(http_recall.get("row_count", 0)) >= 1,
    "grpc_recall_ok": grpc_recall.get("kind") == "query" and int(grpc_recall.get("row_count", 0)) >= 1,
    "latency_effective_candidate_limit": int(latency_bench.get("effective_candidate_limit", 0)),
    "recall_effective_candidate_limit": int(recall_bench.get("effective_candidate_limit", 0)),
    "latency_qps": float(latency_bench.get("qps", 0.0)),
    "recall_qps": float(recall_bench.get("qps", 0.0)),
}
report["effective_limit_order_ok"] = (
    report["latency_effective_candidate_limit"] < report["recall_effective_candidate_limit"]
)
report["pass"] = all([
    report["cli_latency_ok"],
    report["cli_recall_ok"],
    report["http_latency_ok"],
    report["http_recall_ok"],
    report["grpc_recall_ok"],
    report["effective_limit_order_ok"],
])
report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(f"cli_latency_ok={report['cli_latency_ok']}")
print(f"cli_recall_ok={report['cli_recall_ok']}")
print(f"http_latency_ok={report['http_latency_ok']}")
print(f"http_recall_ok={report['http_recall_ok']}")
print(f"grpc_recall_ok={report['grpc_recall_ok']}")
print(f"latency_effective_candidate_limit={report['latency_effective_candidate_limit']}")
print(f"recall_effective_candidate_limit={report['recall_effective_candidate_limit']}")
print(f"pass={report['pass']}")
if not report["pass"]:
    raise SystemExit("s29 query profile assertions failed")
PY

echo "[s29-query-profile-hints-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"
