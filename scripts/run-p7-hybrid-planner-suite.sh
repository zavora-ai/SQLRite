#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p7_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p7_hybrid_planner_suite.log"
BRUTE_JSON="$REPORT_DIR/p7_benchmark_bruteforce_f32.json"
HNSW_JSON="$REPORT_DIR/p7_benchmark_hnsw_f32.json"
SUMMARY_JSON="$REPORT_DIR/p7_hybrid_planner_summary.json"
SUMMARY_MD="$REPORT_DIR/P7.md"
P6_SUMMARY="$REPORT_DIR/p6_quantization_summary.json"

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
run_and_log "$QUALITY_LOG" cargo test hybrid_planner_selects_ -- --nocapture
run_and_log "$SUITE_LOG" cargo test hybrid_search_matches_text_and_vector -- --nocapture
run_and_log "$SUITE_LOG" cargo test rrf_changes_hybrid_ordering -- --nocapture

run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --index-mode brute_force --storage-kind f32 --output "$BRUTE_JSON"
run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 --candidate-limit 200 --top-k 10 --index-mode hnsw_baseline --storage-kind f32 --output "$HNSW_JSON"

python3 - "$BRUTE_JSON" "$HNSW_JSON" "$P6_SUMMARY" "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import json
import pathlib
import sys

brute = json.loads(pathlib.Path(sys.argv[1]).read_text())
hnsw = json.loads(pathlib.Path(sys.argv[2]).read_text())
p6_path = pathlib.Path(sys.argv[3])
summary_json = pathlib.Path(sys.argv[4])
summary_md = pathlib.Path(sys.argv[5])

p6 = json.loads(p6_path.read_text()) if p6_path.exists() else None
p6_brute = p6["modes"]["brute_force"]["f32"] if p6 else None
p6_hnsw = p6["modes"]["hnsw_baseline"]["f32"] if p6 else None

summary = {
    "suite": "p7_hybrid_planner",
    "quality_gates_pass": True,
    "brute_force": {
        "qps": brute["qps"],
        "p95_ms": brute["latency"]["p95_ms"],
        "top1_hit_rate": brute["top1_hit_rate"],
        "delta_vs_p6_f32_qps": None if not p6_brute else brute["qps"] - p6_brute["qps"],
        "delta_vs_p6_f32_p95_ms": None if not p6_brute else p6_brute["p95_ms"] - brute["latency"]["p95_ms"],
    },
    "hnsw_baseline": {
        "qps": hnsw["qps"],
        "p95_ms": hnsw["latency"]["p95_ms"],
        "top1_hit_rate": hnsw["top1_hit_rate"],
        "delta_vs_p6_f32_qps": None if not p6_hnsw else hnsw["qps"] - p6_hnsw["qps"],
        "delta_vs_p6_f32_p95_ms": None if not p6_hnsw else p6_hnsw["p95_ms"] - hnsw["latency"]["p95_ms"],
    },
    "delta_hnsw_vs_bruteforce": {
        "qps": hnsw["qps"] - brute["qps"],
        "p95_ms": brute["latency"]["p95_ms"] - hnsw["latency"]["p95_ms"],
    },
}
summary_json.write_text(json.dumps(summary, indent=2) + "\n")
summary_md.write_text(
    "# P7 Hybrid Planner Report\n\n"
    "| Mode | QPS | p95 ms | Top1 hit rate | Delta vs P6 QPS | Delta vs P6 p95 ms |\n"
    "|---|---:|---:|---:|---:|---:|\n"
    f"| brute_force | {summary['brute_force']['qps']:.2f} | {summary['brute_force']['p95_ms']:.4f} | {summary['brute_force']['top1_hit_rate']:.4f} | {summary['brute_force']['delta_vs_p6_f32_qps']:.2f} | {summary['brute_force']['delta_vs_p6_f32_p95_ms']:.4f} |\n"
    f"| hnsw_baseline | {summary['hnsw_baseline']['qps']:.2f} | {summary['hnsw_baseline']['p95_ms']:.4f} | {summary['hnsw_baseline']['top1_hit_rate']:.4f} | {summary['hnsw_baseline']['delta_vs_p6_f32_qps']:.2f} | {summary['hnsw_baseline']['delta_vs_p6_f32_p95_ms']:.4f} |\n\n"
    f"HNSW QPS delta vs brute force: {summary['delta_hnsw_vs_bruteforce']['qps']:.2f}\n\n"
    f"HNSW p95 gain vs brute force (ms): {summary['delta_hnsw_vs_bruteforce']['p95_ms']:.4f}\n"
)
PY

echo "P7 hybrid planner suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- brute force benchmark: $BRUTE_JSON"
echo "- hnsw benchmark: $HNSW_JSON"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
