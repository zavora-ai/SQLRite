#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p8_filtered_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p8_filtered_suite.log"
BRUTE_JSON="$REPORT_DIR/p8_filtered_benchmark_bruteforce_f32.json"
HNSW_JSON="$REPORT_DIR/p8_filtered_benchmark_hnsw_f32.json"
SUMMARY_JSON="$REPORT_DIR/p8_filtered_summary.json"
SUMMARY_MD="$REPORT_DIR/P8_filtered.md"

mkdir -p "$REPORT_DIR"
: > "$QUALITY_LOG"
: > "$SUITE_LOG"

run_and_log() {
  local log_file="$1"
  shift
  printf '\n$ %s\n' "$*" | tee -a "$log_file"
  "$@" 2>&1 | tee -a "$log_file"
}

run_and_log "$QUALITY_LOG" cargo fmt --all --check
run_and_log "$QUALITY_LOG" cargo test benchmark_args_parse_tenant_filter_flags -- --nocapture
run_and_log "$QUALITY_LOG" cargo test filtered_chunk_ids_uses_in_memory_doc_and_metadata_index -- --nocapture
run_and_log "$QUALITY_LOG" cargo test hnsw_vector_search_applies_metadata_filter -- --nocapture

run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --tenant-filters --tenant-count 4 --index-mode brute_force --storage-kind f32 --output "$BRUTE_JSON"
run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --tenant-filters --tenant-count 4 --index-mode hnsw_baseline --storage-kind f32 --output "$HNSW_JSON"

python3 - "$BRUTE_JSON" "$HNSW_JSON" "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import json
import pathlib
import sys

brute = json.loads(pathlib.Path(sys.argv[1]).read_text())
hnsw = json.loads(pathlib.Path(sys.argv[2]).read_text())
summary_json = pathlib.Path(sys.argv[3])
summary_md = pathlib.Path(sys.argv[4])

summary = {
    "suite": "p8_filtered_benchmark",
    "tenant_filters": brute.get("use_tenant_filters", False) and hnsw.get("use_tenant_filters", False),
    "tenant_count": brute.get("tenant_count", 0),
    "brute_force": {
        "qps": brute["qps"],
        "p95_ms": brute["latency"]["p95_ms"],
        "top1_hit_rate": brute["top1_hit_rate"],
    },
    "hnsw_baseline": {
        "qps": hnsw["qps"],
        "p95_ms": hnsw["latency"]["p95_ms"],
        "top1_hit_rate": hnsw["top1_hit_rate"],
    },
    "delta_hnsw_vs_bruteforce": {
        "qps": hnsw["qps"] - brute["qps"],
        "p95_ms": brute["latency"]["p95_ms"] - hnsw["latency"]["p95_ms"],
    },
}
summary_json.write_text(json.dumps(summary, indent=2) + "\n")
summary_md.write_text(
    "# P8 Filtered Benchmark Report\n\n"
    f"Tenant filters enabled: `{summary['tenant_filters']}` with `{summary['tenant_count']}` tenants.\n\n"
    "| Mode | QPS | p95 ms | Top1 hit rate |\n"
    "|---|---:|---:|---:|\n"
    f"| brute_force | {summary['brute_force']['qps']:.2f} | {summary['brute_force']['p95_ms']:.4f} | {summary['brute_force']['top1_hit_rate']:.4f} |\n"
    f"| hnsw_baseline | {summary['hnsw_baseline']['qps']:.2f} | {summary['hnsw_baseline']['p95_ms']:.4f} | {summary['hnsw_baseline']['top1_hit_rate']:.4f} |\n\n"
    f"HNSW QPS delta vs brute force: {summary['delta_hnsw_vs_bruteforce']['qps']:.2f}\n\n"
    f"HNSW p95 gain vs brute force (ms): {summary['delta_hnsw_vs_bruteforce']['p95_ms']:.4f}\n"
)
PY

echo "P8 filtered benchmark suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- brute force benchmark: $BRUTE_JSON"
echo "- hnsw benchmark: $HNSW_JSON"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
