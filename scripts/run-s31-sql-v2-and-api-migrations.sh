#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
REPORT_DIR="${REPORT_DIR:-project_plan/reports}"
LOG_PATH="${LOG_PATH:-$REPORT_DIR/s31_sql_v2_migration.log}"
REPORT_PATH="${REPORT_PATH:-$REPORT_DIR/s31_sql_v2_migration_report.json}"
BENCH_PATH="${BENCH_PATH:-$REPORT_DIR/s31_benchmark_search_v2.json}"
QUALITY_LOG_PATH="${QUALITY_LOG_PATH:-$REPORT_DIR/s31_quality_gates.log}"
GAP_PATH="${GAP_PATH:-$REPORT_DIR/s31_competitor_gap_analysis.md}"
WORK_DIR="${WORK_DIR:-$REPORT_DIR/s31_fixture_workspace}"
HTTP_BIND_ADDR="${HTTP_BIND_ADDR:-127.0.0.1:8351}"
KEEP_WORK_DIR="${KEEP_WORK_DIR:-0}"

mkdir -p "$REPORT_DIR"
rm -f "$LOG_PATH" "$REPORT_PATH" "$BENCH_PATH"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_WORK_DIR" != "1" ]]; then
    rm -rf "$WORK_DIR"
  fi
}
trap cleanup EXIT

QDRANT_INPUT="$WORK_DIR/qdrant_export.jsonl"
WEAVIATE_INPUT="$WORK_DIR/weaviate_export.jsonl"
MILVUS_INPUT="$WORK_DIR/milvus_export.jsonl"
QDRANT_DB="$WORK_DIR/qdrant.db"
WEAVIATE_DB="$WORK_DIR/weaviate.db"
MILVUS_DB="$WORK_DIR/milvus.db"
QDRANT_JSON="$WORK_DIR/qdrant_migrate.json"
WEAVIATE_JSON="$WORK_DIR/weaviate_migrate.json"
MILVUS_JSON="$WORK_DIR/milvus_migrate.json"
QDRANT_DOCTOR_JSON="$WORK_DIR/qdrant_doctor.json"
WEAVIATE_DOCTOR_JSON="$WORK_DIR/weaviate_doctor.json"
MILVUS_DOCTOR_JSON="$WORK_DIR/milvus_doctor.json"
CLI_SEARCH_OUT="$WORK_DIR/cli_search.json"
HTTP_SQL_OUT="$WORK_DIR/http_sql.json"
HTTP_RERANK_OUT="$WORK_DIR/http_rerank.json"
SERVER_LOG="$WORK_DIR/server.log"

exec > >(tee -a "$LOG_PATH") 2>&1

echo "[build]"
cargo build --bin sqlrite >/dev/null

echo "[quality-gates]"
{
  echo '$ cargo fmt --all --check'
  cargo fmt --all --check
  echo '$ cargo test'
  cargo test
  echo '$ bash scripts/run-s31-sql-v2-and-api-migrations.sh'
  echo 'invoked from quality gate runner'
} > "$QUALITY_LOG_PATH" 2>&1

echo "[create-fixtures] work_dir=$WORK_DIR"
python3 - <<'PY' "$QDRANT_INPUT" "$WEAVIATE_INPUT" "$MILVUS_INPUT"
import json
import sys
from pathlib import Path

qdrant_path = Path(sys.argv[1])
weaviate_path = Path(sys.argv[2])
milvus_path = Path(sys.argv[3])

qdrant_rows = [
    {
        "id": "qd-1",
        "payload": {
            "doc_id": "doc-1",
            "content": "qdrant agent memory chunk",
            "source": "qdrant/doc-1.md",
            "tenant": "demo",
            "topic": "memory",
        },
        "vector": [0.91, 0.09, 0.0],
    },
    {
        "id": "qd-2",
        "payload": {
            "doc_id": "doc-2",
            "content": "qdrant retrieval chunk",
            "source": "qdrant/doc-2.md",
            "tenant": "demo",
            "topic": "retrieval",
        },
        "vector": {"default": [0.75, 0.25, 0.0]},
    },
]
weaviate_rows = [
    {
        "id": "wv-1",
        "properties": {
            "doc_id": "doc-1",
            "content": "weaviate agent memory chunk",
            "source": "weaviate/doc-1.md",
            "tenant": "demo",
        },
        "vector": [0.88, 0.12, 0.0],
    }
]
milvus_rows = [
    {
        "id": "mv-1",
        "doc_id": "doc-1",
        "content": "milvus agent memory chunk",
        "source": "milvus/doc-1.md",
        "metadata": {"tenant": "demo", "topic": "memory"},
        "embedding": [0.86, 0.14, 0.0],
    }
]
for path, rows in [(qdrant_path, qdrant_rows), (weaviate_path, weaviate_rows), (milvus_path, milvus_rows)]:
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row) + "\n")
PY

echo "[migrate-qdrant]"
"$BIN" migrate qdrant --input "$QDRANT_INPUT" --target "$QDRANT_DB" --index-mode hnsw_baseline --create-indexes --json > "$QDRANT_JSON"
"$BIN" doctor --db "$QDRANT_DB" --index-mode hnsw_baseline --json > "$QDRANT_DOCTOR_JSON"

echo "[migrate-weaviate]"
"$BIN" migrate weaviate --input "$WEAVIATE_INPUT" --target "$WEAVIATE_DB" --index-mode hnsw_baseline --create-indexes --json > "$WEAVIATE_JSON"
"$BIN" doctor --db "$WEAVIATE_DB" --index-mode hnsw_baseline --json > "$WEAVIATE_DOCTOR_JSON"

echo "[migrate-milvus]"
"$BIN" migrate milvus --input "$MILVUS_INPUT" --target "$MILVUS_DB" --index-mode hnsw_baseline --create-indexes --json > "$MILVUS_JSON"
"$BIN" doctor --db "$MILVUS_DB" --index-mode hnsw_baseline --json > "$MILVUS_DOCTOR_JSON"

echo "[cli-search-v2]"
"$BIN" sql --db "$QDRANT_DB" --execute "SELECT chunk_id, doc_id, hybrid_score FROM SEARCH('agent memory', vector('0.91,0.09,0.0'), 5, 0.65, 500, 'latency', '{\"tenant\":\"demo\"}', NULL) ORDER BY hybrid_score DESC, chunk_id ASC;" > "$CLI_SEARCH_OUT"

echo "[start-server] bind=$HTTP_BIND_ADDR"
"$BIN" serve --db "$QDRANT_DB" --bind "$HTTP_BIND_ADDR" > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 120); do
  if curl -fsS "http://$HTTP_BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$HTTP_BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "server did not become ready"
  tail -n 120 "$SERVER_LOG" || true
  exit 1
fi

echo "[http-sql-search-v2]"
HTTP_SQL_PAYLOAD="$(python3 - <<'PY'
import json
statement = "SELECT chunk_id, doc_id, hybrid_score FROM SEARCH('agent memory', vector('0.91,0.09,0.0'), 5, 0.65, 500, 'recall', NULL, NULL) ORDER BY hybrid_score DESC, chunk_id ASC;"
print(json.dumps({"statement": statement}))
PY
)"
curl -fsS -X POST \
  -H 'content-type: application/json' \
  -d "$HTTP_SQL_PAYLOAD" \
  "http://$HTTP_BIND_ADDR/v1/sql" > "$HTTP_SQL_OUT"

echo "[http-rerank-hook]"
curl -fsS -X POST \
  -H 'content-type: application/json' \
  -d '{"query_text":"agent memory","candidate_count":5,"query_profile":"recall"}' \
  "http://$HTTP_BIND_ADDR/v1/rerank-hook" > "$HTTP_RERANK_OUT"

echo "[gap-analysis]"
cat > "$GAP_PATH" <<'MD'
# S31 Competitor Gap Analysis

Date: March 7, 2026

## Focus of this review

This review is limited to roadmap scope covered in S31:

- API-first vector database migration ergonomics
- concise SQL-native retrieval syntax
- rerank-ready and query-profile-aware SQL/server workflows

## Shipped in S31

- native JSONL import commands for Qdrant, Weaviate, and Milvus export shapes
- `SEARCH(...)` SQL v2 prototype in CLI SQL mode and server `/v1/sql`
- validation harness covering API-first migration, SQL v2, and rerank-hook compatibility

## Remaining gaps after S31

- no direct network pull connectors from remote Qdrant, Weaviate, or Milvus clusters
- `SEARCH(...)` is a rewrite-based prototype, not a true SQLite virtual table module
- no built-in cross-encoder reranker packaged in-process yet
- no source-specific export assistants for managed vendor backup formats yet

## Target follow-through

- S32+: release-hardening and defect burn-down for SQL v2 semantics
- post-v1: native remote export connectors and richer `SEARCH` syntax
MD

echo "[assertions]"
python3 - <<'PY' \
  "$QDRANT_JSON" \
  "$WEAVIATE_JSON" \
  "$MILVUS_JSON" \
  "$QDRANT_DOCTOR_JSON" \
  "$WEAVIATE_DOCTOR_JSON" \
  "$MILVUS_DOCTOR_JSON" \
  "$CLI_SEARCH_OUT" \
  "$HTTP_SQL_OUT" \
  "$HTTP_RERANK_OUT" \
  "$REPORT_PATH" \
  "$BENCH_PATH"
import json
import os
import platform
import sys
import time
from pathlib import Path

qdrant = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
weaviate = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
milvus = json.loads(Path(sys.argv[3]).read_text(encoding="utf-8"))
qdrant_doctor = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
weaviate_doctor = json.loads(Path(sys.argv[5]).read_text(encoding="utf-8"))
milvus_doctor = json.loads(Path(sys.argv[6]).read_text(encoding="utf-8"))
cli_search = json.loads(Path(sys.argv[7]).read_text(encoding="utf-8"))
http_sql = json.loads(Path(sys.argv[8]).read_text(encoding="utf-8"))
http_rerank = json.loads(Path(sys.argv[9]).read_text(encoding="utf-8"))
report_path = Path(sys.argv[10])
bench_path = Path(sys.argv[11])

hardware = {
    "platform": platform.system().lower(),
    "machine": platform.machine().lower(),
    "cpu_count": os.cpu_count() or 1,
}
hardware_class = f"{hardware['platform']}-{hardware['machine']}-{hardware['cpu_count']}cpu"

def rows_per_second(report):
    duration_ms = max(float(report.get("duration_ms", 0.0)), 0.001)
    return round(float(report.get("chunks_migrated", 0)) / (duration_ms / 1000.0), 4)

summary = {
    "generated_unix_ms": int(time.time() * 1000),
    "embedding_model": "fixture_f32le_v1",
    "dataset_id": "s31_api_first_fixture_v1",
    "hardware_class": hardware_class,
    "hardware": hardware,
    "qdrant_ok": qdrant.get("kind") == "qdrant_jsonl" and int(qdrant.get("chunks_migrated", 0)) == 2 and int(qdrant_doctor["db"]["chunk_count"]) == 2 and bool(qdrant_doctor["db"]["integrity_ok"]),
    "weaviate_ok": weaviate.get("kind") == "weaviate_jsonl" and int(weaviate.get("chunks_migrated", 0)) == 1 and int(weaviate_doctor["db"]["chunk_count"]) == 1 and bool(weaviate_doctor["db"]["integrity_ok"]),
    "milvus_ok": milvus.get("kind") == "milvus_jsonl" and int(milvus.get("chunks_migrated", 0)) == 1 and int(milvus_doctor["db"]["chunk_count"]) == 1 and bool(milvus_doctor["db"]["integrity_ok"]),
    "cli_search_ok": isinstance(cli_search, list) and len(cli_search) >= 1 and cli_search[0].get("chunk_id") == "qd-1",
    "http_sql_ok": http_sql.get("kind") == "query" and int(http_sql.get("row_count", 0)) >= 1 and http_sql["rows"][0].get("chunk_id") == "qd-1",
    "rerank_hook_ok": http_rerank.get("kind") == "rerank_hook" and int(http_rerank.get("row_count", 0)) >= 1,
    "qdrant_rows_per_sec": rows_per_second(qdrant),
    "weaviate_rows_per_sec": rows_per_second(weaviate),
    "milvus_rows_per_sec": rows_per_second(milvus),
    "http_sql_elapsed_ms": float(http_sql.get("elapsed_ms", 0.0)),
}
summary["pass"] = all([
    summary["qdrant_ok"],
    summary["weaviate_ok"],
    summary["milvus_ok"],
    summary["cli_search_ok"],
    summary["http_sql_ok"],
    summary["rerank_hook_ok"],
])

benchmark = {
    "generated_unix_ms": summary["generated_unix_ms"],
    "embedding_model": summary["embedding_model"],
    "dataset_id": summary["dataset_id"],
    "hardware_class": summary["hardware_class"],
    "hardware": summary["hardware"],
    "qdrant_duration_ms": float(qdrant.get("duration_ms", 0.0)),
    "qdrant_rows_per_sec": summary["qdrant_rows_per_sec"],
    "weaviate_duration_ms": float(weaviate.get("duration_ms", 0.0)),
    "weaviate_rows_per_sec": summary["weaviate_rows_per_sec"],
    "milvus_duration_ms": float(milvus.get("duration_ms", 0.0)),
    "milvus_rows_per_sec": summary["milvus_rows_per_sec"],
    "http_sql_elapsed_ms": summary["http_sql_elapsed_ms"],
    "rerank_row_count": int(http_rerank.get("row_count", 0)),
}

report_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
bench_path.write_text(json.dumps(benchmark, indent=2) + "\n", encoding="utf-8")
print(f"qdrant_ok={summary['qdrant_ok']}")
print(f"weaviate_ok={summary['weaviate_ok']}")
print(f"milvus_ok={summary['milvus_ok']}")
print(f"cli_search_ok={summary['cli_search_ok']}")
print(f"http_sql_ok={summary['http_sql_ok']}")
print(f"rerank_hook_ok={summary['rerank_hook_ok']}")
print(f"qdrant_rows_per_sec={summary['qdrant_rows_per_sec']}")
print(f"http_sql_elapsed_ms={summary['http_sql_elapsed_ms']}")
print(f"pass={summary['pass']}")
if not summary["pass"]:
    raise SystemExit("s31 assertions failed")
PY

echo "[s31-complete] report=$REPORT_PATH benchmark=$BENCH_PATH gap=$GAP_PATH"
