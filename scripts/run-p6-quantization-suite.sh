#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p6_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p6_quantization_suite.log"
SUMMARY_JSON="$REPORT_DIR/p6_quantization_summary.json"
SUMMARY_MD="$REPORT_DIR/P6.md"

mkdir -p "$REPORT_DIR"
: > "$QUALITY_LOG"
: > "$SUITE_LOG"

run_and_log() {
  local log_file="$1"
  shift
  printf '\n$ %s\n' "$*" | tee -a "$log_file"
  "$@" 2>&1 | tee -a "$log_file"
}

run_bench() {
  local mode="$1"
  local storage="$2"
  local output="$REPORT_DIR/p6_benchmark_${mode}_${storage}.json"
  run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- \
    --corpus 5000 --queries 150 --warmup 30 --embedding-dim 64 \
    --candidate-limit 200 --top-k 10 --index-mode "$mode" --storage-kind "$storage" \
    --output "$output"
}

run_and_log "$QUALITY_LOG" cargo fmt --all --check
run_and_log "$QUALITY_LOG" cargo test quantized_storage_preserves_ranking -- --nocapture
run_and_log "$SUITE_LOG" cargo test ann_snapshot_round_trip_int8_precision -- --nocapture
run_and_log "$SUITE_LOG" cargo test exact_segment_snapshot_round_trip_int8_precision -- --nocapture
run_and_log "$SUITE_LOG" cargo test ann_snapshot_persists_for_file_backed_ann_index -- --nocapture
run_and_log "$SUITE_LOG" cargo test exact_segment_persists_for_file_backed_bruteforce_index -- --nocapture

for mode in brute_force hnsw_baseline; do
  for storage in f32 f16 int8; do
    run_bench "$mode" "$storage"
  done
done

python3 - "$REPORT_DIR" "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import json
import pathlib
import sys

report_dir = pathlib.Path(sys.argv[1])
summary_json = pathlib.Path(sys.argv[2])
summary_md = pathlib.Path(sys.argv[3])

modes = ["brute_force", "hnsw_baseline"]
storage_kinds = ["f32", "f16", "int8"]
reports = {}
for mode in modes:
    reports[mode] = {}
    for storage in storage_kinds:
        path = report_dir / f"p6_benchmark_{mode}_{storage}.json"
        reports[mode][storage] = json.loads(path.read_text())

summary = {
    "suite": "p6_quantization",
    "quality_gates_pass": True,
    "modes": {},
}

for mode in modes:
    baseline = reports[mode]["f32"]
    summary["modes"][mode] = {}
    for storage in storage_kinds:
        report = reports[mode][storage]
        summary["modes"][mode][storage] = {
            "qps": report["qps"],
            "p95_ms": report["latency"]["p95_ms"],
            "top1_hit_rate": report["top1_hit_rate"],
            "estimated_memory_bytes": report["vector_index_estimated_memory_bytes"],
            "qps_delta_vs_f32": report["qps"] - baseline["qps"],
            "memory_delta_vs_f32": report["vector_index_estimated_memory_bytes"] - baseline["vector_index_estimated_memory_bytes"],
        }

summary_json.write_text(json.dumps(summary, indent=2) + "\n")

lines = [
    "# P6 Quantization Report",
    "",
    "| Mode | Storage | QPS | p95 ms | Top1 hit rate | Est. memory bytes | QPS delta vs f32 | Memory delta vs f32 |",
    "|---|---|---:|---:|---:|---:|---:|---:|",
]
for mode in modes:
    for storage in storage_kinds:
        item = summary["modes"][mode][storage]
        lines.append(
            f"| {mode} | {storage} | {item['qps']:.2f} | {item['p95_ms']:.4f} | {item['top1_hit_rate']:.4f} | {item['estimated_memory_bytes']} | {item['qps_delta_vs_f32']:.2f} | {item['memory_delta_vs_f32']} |"
        )
    lines.append("")
summary_md.write_text("\n".join(lines) + "\n")
PY

echo "P6 quantization suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
