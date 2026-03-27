#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p0_p1_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p0_p1_sidecar_suite.log"
BRUTE_JSON="$REPORT_DIR/p0_p1_benchmark_bruteforce.json"
HNSW_JSON="$REPORT_DIR/p0_p1_benchmark_hnsw.json"
SUMMARY_JSON="$REPORT_DIR/p0_p1_summary.json"
SUMMARY_MD="$REPORT_DIR/P0_P1.md"

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
run_and_log "$QUALITY_LOG" cargo test hnsw_baseline_ -- --nocapture

run_and_log "$SUITE_LOG" cargo test ann_snapshot_persists_for_file_backed_ann_index -- --nocapture
run_and_log "$SUITE_LOG" cargo test file_backed_ann_reopen_prefers_binary_entry_sidecar -- --nocapture
run_and_log "$SUITE_LOG" cargo test exact_segment_persists_for_file_backed_bruteforce_index -- --nocapture
run_and_log "$SUITE_LOG" cargo test file_backed_bruteforce_reopen_prefers_exact_segment_sidecar -- --nocapture

run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --index-mode brute_force --output "$BRUTE_JSON"
run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --index-mode hnsw_baseline --output "$HNSW_JSON"

python3 - "$BRUTE_JSON" "$HNSW_JSON" "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import json
import pathlib
import sys

brute_path = pathlib.Path(sys.argv[1])
hnsw_path = pathlib.Path(sys.argv[2])
summary_path = pathlib.Path(sys.argv[3])
summary_md_path = pathlib.Path(sys.argv[4])

brute = json.loads(brute_path.read_text())
hnsw = json.loads(hnsw_path.read_text())
summary = {
    "suite": "p0_p1_performance",
    "quality_gates_pass": True,
    "sidecar_smoke_pass": True,
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
    "delta": {
        "qps_gain_vs_bruteforce": hnsw["qps"] - brute["qps"],
        "p95_ms_gain_vs_bruteforce": brute["latency"]["p95_ms"] - hnsw["latency"]["p95_ms"],
    },
}
summary_path.write_text(json.dumps(summary, indent=2) + "\n")
summary_md_path.write_text(
    "# P0/P1 Performance Report\n\n"
    "| Mode | QPS | p95 ms | Top1 hit rate |\n"
    "|---|---:|---:|---:|\n"
    f"| brute_force | {brute['qps']:.2f} | {brute['latency']['p95_ms']:.4f} | {brute['top1_hit_rate']:.4f} |\n"
    f"| hnsw_baseline | {hnsw['qps']:.2f} | {hnsw['latency']['p95_ms']:.4f} | {hnsw['top1_hit_rate']:.4f} |\n\n"
    f"HNSW QPS delta vs brute force: {summary['delta']['qps_gain_vs_bruteforce']:.2f}\n\n"
    f"HNSW p95 gain vs brute force (ms): {summary['delta']['p95_ms_gain_vs_bruteforce']:.4f}\n"
)
PY

echo "P0/P1 performance suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- sidecar log: $SUITE_LOG"
echo "- brute force benchmark: $BRUTE_JSON"
echo "- hnsw benchmark: $HNSW_JSON"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
