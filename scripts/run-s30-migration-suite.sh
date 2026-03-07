#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
REPORT_DIR="${REPORT_DIR:-project_plan/reports}"
LOG_PATH="${LOG_PATH:-$REPORT_DIR/s30_migration_suite.log}"
REPORT_PATH="${REPORT_PATH:-$REPORT_DIR/s30_migration_report.json}"
BENCH_PATH="${BENCH_PATH:-$REPORT_DIR/s30_benchmark_migration.json}"
QUALITY_LOG_PATH="${QUALITY_LOG_PATH:-$REPORT_DIR/s30_quality_gates.log}"
WORK_DIR="${WORK_DIR:-$REPORT_DIR/s30_fixture_workspace}"
KEEP_WORK_DIR="${KEEP_WORK_DIR:-0}"

mkdir -p "$REPORT_DIR"
rm -f "$LOG_PATH" "$REPORT_PATH" "$BENCH_PATH"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

cleanup() {
  if [[ "$KEEP_WORK_DIR" != "1" ]]; then
    rm -rf "$WORK_DIR"
  fi
}
trap cleanup EXIT

SQLITE_SOURCE="$WORK_DIR/legacy_sqlite.db"
PGVECTOR_INPUT="$WORK_DIR/pgvector_export.jsonl"
SQLITE_TARGET="$WORK_DIR/sqlite_migrated.db"
LIBSQL_TARGET="$WORK_DIR/libsql_migrated.db"
PGVECTOR_TARGET="$WORK_DIR/pgvector_migrated.db"

SQLITE_JSON="$WORK_DIR/sqlite_migrate.json"
LIBSQL_JSON="$WORK_DIR/libsql_migrate.json"
PGVECTOR_JSON="$WORK_DIR/pgvector_migrate.json"
SQLITE_DOCTOR_JSON="$WORK_DIR/sqlite_doctor.json"
LIBSQL_DOCTOR_JSON="$WORK_DIR/libsql_doctor.json"
PGVECTOR_DOCTOR_JSON="$WORK_DIR/pgvector_doctor.json"
SQLITE_QUERY_OUT="$WORK_DIR/sqlite_query.txt"
LIBSQL_QUERY_OUT="$WORK_DIR/libsql_query.txt"
PGVECTOR_QUERY_OUT="$WORK_DIR/pgvector_query.txt"

exec > >(tee -a "$LOG_PATH") 2>&1

echo "[build]"
cargo build --bin sqlrite >/dev/null

echo "[quality-gates]"
{
  echo '$ cargo fmt --all --check'
  cargo fmt --all --check
  echo '$ cargo test'
  cargo test
  echo '$ bash scripts/run-s30-migration-suite.sh'
  echo 'invoked from quality gate runner'
} > "$QUALITY_LOG_PATH" 2>&1

echo "[create-fixtures] work_dir=$WORK_DIR"
python3 - <<'PY' "$SQLITE_SOURCE" "$PGVECTOR_INPUT"
import json
import sqlite3
import sys
from pathlib import Path

sqlite_path = Path(sys.argv[1])
jsonl_path = Path(sys.argv[2])

conn = sqlite3.connect(sqlite_path)
conn.executescript(
    """
    CREATE TABLE legacy_documents (
        doc_id TEXT PRIMARY KEY,
        source_path TEXT,
        metadata_json TEXT
    );
    CREATE TABLE legacy_chunks (
        chunk_id TEXT PRIMARY KEY,
        doc_id TEXT NOT NULL,
        chunk_text TEXT NOT NULL,
        metadata_json TEXT,
        embedding_blob BLOB,
        embedding_dim INTEGER,
        source_path TEXT
    );
    CREATE TABLE csv_chunks (
        chunk_id TEXT PRIMARY KEY,
        doc_id TEXT NOT NULL,
        chunk_text TEXT NOT NULL,
        metadata_json TEXT,
        embedding_csv TEXT NOT NULL,
        source_path TEXT
    );
    """
)

def blob(values):
    import struct
    return b"".join(struct.pack("<f", value) for value in values)

legacy_docs = [
    ("doc-1", "legacy/doc-1.md", json.dumps({"tenant": "acme", "corpus": "sqlite"})),
    ("doc-2", "legacy/doc-2.md", json.dumps({"tenant": "acme", "corpus": "sqlite"})),
]
legacy_chunks = [
    (
        "chunk-1",
        "doc-1",
        "sqlite agent memory chunk",
        json.dumps({"topic": "memory", "source": "sqlite"}),
        blob([0.90, 0.10, 0.00]),
        3,
        "legacy/doc-1.md",
    ),
    (
        "chunk-2",
        "doc-2",
        "sqlite retrieval tuning chunk",
        json.dumps({"topic": "retrieval", "source": "sqlite"}),
        blob([0.20, 0.80, 0.00]),
        3,
        "legacy/doc-2.md",
    ),
]
conn.executemany(
    "INSERT INTO legacy_documents (doc_id, source_path, metadata_json) VALUES (?, ?, ?)",
    legacy_docs,
)
conn.executemany(
    "INSERT INTO legacy_chunks (chunk_id, doc_id, chunk_text, metadata_json, embedding_blob, embedding_dim, source_path) VALUES (?, ?, ?, ?, ?, ?, ?)",
    legacy_chunks,
)
conn.executemany(
    "INSERT INTO csv_chunks (chunk_id, doc_id, chunk_text, metadata_json, embedding_csv, source_path) VALUES (?, ?, ?, ?, ?, ?)",
    [
        (
            "csv-1",
            "doc-1",
            "csv migration path chunk",
            json.dumps({"topic": "csv", "source": "sqlite"}),
            "0.5,0.5,0.0",
            "legacy/doc-1.md",
        )
    ],
)
conn.commit()
conn.close()

records = [
    {
        "id": "pg-1",
        "doc_id": "pg-doc-1",
        "content": "pgvector migrated chunk",
        "metadata": {"tenant": "acme", "source": "pgvector"},
        "embedding": [0.88, 0.12, 0.00],
        "source": "pg/doc-1.md",
        "doc_metadata": {"tenant": "acme", "corpus": "pgvector"},
        "doc_source": "pg/doc-1.md",
    },
    {
        "id": "pg-2",
        "doc_id": "pg-doc-2",
        "content": "api first export normalized chunk",
        "metadata": {"tenant": "acme", "source": "api-first"},
        "embedding": [0.32, 0.68, 0.00],
        "source": "pg/doc-2.md",
        "doc_metadata": {"tenant": "acme", "corpus": "pgvector"},
        "doc_source": "pg/doc-2.md",
    },
]
with jsonl_path.open("w", encoding="utf-8") as handle:
    for record in records:
        handle.write(json.dumps(record) + "\n")
PY

echo "[migrate-sqlite]"
"$BIN" migrate sqlite \
  --source "$SQLITE_SOURCE" \
  --target "$SQLITE_TARGET" \
  --profile balanced \
  --index-mode hnsw_baseline \
  --batch-size 2 \
  --create-indexes \
  --json > "$SQLITE_JSON"

"$BIN" doctor --db "$SQLITE_TARGET" --index-mode hnsw_baseline --json > "$SQLITE_DOCTOR_JSON"
"$BIN" query --db "$SQLITE_TARGET" --index-mode hnsw_baseline --text "agent memory" --top-k 2 > "$SQLITE_QUERY_OUT"

echo "[migrate-libsql-alias]"
"$BIN" migrate libsql \
  --source "$SQLITE_SOURCE" \
  --target "$LIBSQL_TARGET" \
  --profile balanced \
  --index-mode hnsw_baseline \
  --doc-table none \
  --chunk-table csv_chunks \
  --chunk-embedding-col embedding_csv \
  --chunk-embedding-dim-col none \
  --chunk-source-col source_path \
  --embedding-format csv \
  --batch-size 1 \
  --json > "$LIBSQL_JSON"

"$BIN" doctor --db "$LIBSQL_TARGET" --index-mode hnsw_baseline --json > "$LIBSQL_DOCTOR_JSON"
"$BIN" query --db "$LIBSQL_TARGET" --index-mode hnsw_baseline --text "csv migration" --top-k 2 > "$LIBSQL_QUERY_OUT"

echo "[migrate-pgvector-jsonl]"
"$BIN" migrate pgvector \
  --input "$PGVECTOR_INPUT" \
  --target "$PGVECTOR_TARGET" \
  --profile balanced \
  --index-mode hnsw_baseline \
  --batch-size 2 \
  --create-indexes \
  --json > "$PGVECTOR_JSON"

"$BIN" doctor --db "$PGVECTOR_TARGET" --index-mode hnsw_baseline --json > "$PGVECTOR_DOCTOR_JSON"
"$BIN" query --db "$PGVECTOR_TARGET" --index-mode hnsw_baseline --text "pgvector migrated" --top-k 2 > "$PGVECTOR_QUERY_OUT"

echo "[assertions]"
python3 - <<'PY' \
  "$SQLITE_JSON" \
  "$LIBSQL_JSON" \
  "$PGVECTOR_JSON" \
  "$SQLITE_DOCTOR_JSON" \
  "$LIBSQL_DOCTOR_JSON" \
  "$PGVECTOR_DOCTOR_JSON" \
  "$SQLITE_QUERY_OUT" \
  "$LIBSQL_QUERY_OUT" \
  "$PGVECTOR_QUERY_OUT" \
  "$REPORT_PATH" \
  "$BENCH_PATH"
import json
import os
import platform
import sys
import time
from pathlib import Path

sqlite_report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
libsql_report = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
pgvector_report = json.loads(Path(sys.argv[3]).read_text(encoding="utf-8"))
sqlite_doctor = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
libsql_doctor = json.loads(Path(sys.argv[5]).read_text(encoding="utf-8"))
pgvector_doctor = json.loads(Path(sys.argv[6]).read_text(encoding="utf-8"))
sqlite_query = Path(sys.argv[7]).read_text(encoding="utf-8")
libsql_query = Path(sys.argv[8]).read_text(encoding="utf-8")
pgvector_query = Path(sys.argv[9]).read_text(encoding="utf-8")
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
    "dataset_id": "s30_migration_fixture_v1",
    "hardware_class": hardware_class,
    "hardware": hardware,
    "sqlite": {
        "report": sqlite_report,
        "doctor_chunk_count": int(sqlite_doctor["db"]["chunk_count"]),
        "query_hit": "chunk-1" in sqlite_query,
        "rows_per_sec": rows_per_second(sqlite_report),
    },
    "libsql_alias": {
        "report": libsql_report,
        "doctor_chunk_count": int(libsql_doctor["db"]["chunk_count"]),
        "query_hit": "csv-1" in libsql_query,
        "rows_per_sec": rows_per_second(libsql_report),
    },
    "pgvector": {
        "report": pgvector_report,
        "doctor_chunk_count": int(pgvector_doctor["db"]["chunk_count"]),
        "query_hit": "pg-1" in pgvector_query,
        "rows_per_sec": rows_per_second(pgvector_report),
    },
}
summary["sqlite"]["ok"] = (
    sqlite_report.get("kind") == "sqlite"
    and int(sqlite_report.get("documents_upserted", 0)) == 2
    and int(sqlite_report.get("chunks_migrated", 0)) == 2
    and summary["sqlite"]["doctor_chunk_count"] == 2
    and bool(sqlite_doctor["db"]["integrity_ok"])
    and summary["sqlite"]["query_hit"]
)
summary["libsql_alias"]["ok"] = (
    libsql_report.get("kind") == "sqlite"
    and int(libsql_report.get("documents_upserted", 0)) == 0
    and int(libsql_report.get("chunks_migrated", 0)) == 1
    and summary["libsql_alias"]["doctor_chunk_count"] == 1
    and bool(libsql_doctor["db"]["integrity_ok"])
    and summary["libsql_alias"]["query_hit"]
)
summary["pgvector"]["ok"] = (
    pgvector_report.get("kind") == "pgvector_jsonl"
    and int(pgvector_report.get("documents_upserted", 0)) == 2
    and int(pgvector_report.get("chunks_migrated", 0)) == 2
    and summary["pgvector"]["doctor_chunk_count"] == 2
    and bool(pgvector_doctor["db"]["integrity_ok"])
    and summary["pgvector"]["query_hit"]
)
summary["pass"] = summary["sqlite"]["ok"] and summary["libsql_alias"]["ok"] and summary["pgvector"]["ok"]

benchmark = {
    "generated_unix_ms": summary["generated_unix_ms"],
    "embedding_model": summary["embedding_model"],
    "dataset_id": summary["dataset_id"],
    "hardware_class": summary["hardware_class"],
    "hardware": summary["hardware"],
    "sqlite_duration_ms": float(sqlite_report.get("duration_ms", 0.0)),
    "sqlite_rows_per_sec": summary["sqlite"]["rows_per_sec"],
    "libsql_alias_duration_ms": float(libsql_report.get("duration_ms", 0.0)),
    "libsql_alias_rows_per_sec": summary["libsql_alias"]["rows_per_sec"],
    "pgvector_duration_ms": float(pgvector_report.get("duration_ms", 0.0)),
    "pgvector_rows_per_sec": summary["pgvector"]["rows_per_sec"],
}

report_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
bench_path.write_text(json.dumps(benchmark, indent=2) + "\n", encoding="utf-8")
print(f"sqlite_ok={summary['sqlite']['ok']}")
print(f"libsql_alias_ok={summary['libsql_alias']['ok']}")
print(f"pgvector_ok={summary['pgvector']['ok']}")
print(f"hardware_class={summary['hardware_class']}")
print(f"sqlite_rows_per_sec={summary['sqlite']['rows_per_sec']}")
print(f"libsql_alias_rows_per_sec={summary['libsql_alias']['rows_per_sec']}")
print(f"pgvector_rows_per_sec={summary['pgvector']['rows_per_sec']}")
print(f"pass={summary['pass']}")
if not summary["pass"]:
    raise SystemExit("s30 migration assertions failed")
PY

echo "[s30-migration-suite-complete] report=$REPORT_PATH benchmark=$BENCH_PATH quality_log=$QUALITY_LOG_PATH"
