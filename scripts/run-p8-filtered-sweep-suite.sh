#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p8_filtered_sweep_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p8_filtered_sweep_suite.log"
SUMMARY_JSON="$REPORT_DIR/p8_filtered_sweep_summary.json"
SUMMARY_MD="$REPORT_DIR/P8_filtered_sweep.md"
RAW_JSON="$REPORT_DIR/p8_filtered_sweep_runs.json"

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
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test benchmark_args_parse_filter_mode -- --nocapture
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test filtered_chunk_ids_uses_in_memory_doc_and_metadata_index -- --nocapture
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test hnsw_vector_search_applies_metadata_filter -- --nocapture

run_case() {
  local index_mode="$1"
  local filter_mode="$2"
  local tenant_count="$3"
  local output_path="$4"

  run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- \
    --corpus 5000 \
    --queries 100 \
    --warmup 20 \
    --embedding-dim 64 \
    --candidate-limit 200 \
    --top-k 10 \
    --filter-mode "$filter_mode" \
    --tenant-count "$tenant_count" \
    --index-mode "$index_mode" \
    --storage-kind f32 \
    --output "$output_path"
}

TENANT_SWEEP_COUNTS=(2 4 8 16)
FILTER_SWEEP_MODES=(tenant topic tenant_and_topic)

raw_args=()

for tenant_count in "${TENANT_SWEEP_COUNTS[@]}"; do
  for index_mode in brute_force hnsw_baseline; do
    output_path="$REPORT_DIR/p8_tenant_sweep_${index_mode}_tenants_${tenant_count}.json"
    run_case "$index_mode" tenant "$tenant_count" "$output_path"
    raw_args+=("$output_path")
  done
done

for filter_mode in "${FILTER_SWEEP_MODES[@]}"; do
  for index_mode in brute_force hnsw_baseline; do
    output_path="$REPORT_DIR/p8_filter_mode_${index_mode}_${filter_mode}.json"
    run_case "$index_mode" "$filter_mode" 8 "$output_path"
    raw_args+=("$output_path")
  done
done

python3 - "$RAW_JSON" "$SUMMARY_JSON" "$SUMMARY_MD" "${raw_args[@]}" <<'PY'
import json
import pathlib
import sys

raw_json = pathlib.Path(sys.argv[1])
summary_json = pathlib.Path(sys.argv[2])
summary_md = pathlib.Path(sys.argv[3])
paths = [pathlib.Path(p) for p in sys.argv[4:]]

runs = [json.loads(path.read_text()) for path in paths]
raw_json.write_text(json.dumps(runs, indent=2) + "\n")

tenant_runs = [
    run for run in runs
    if run.get("filter_mode") == "tenant"
]
filter_runs = [
    run for run in runs
    if run.get("tenant_count") == 8 and run.get("filter_mode") in {"tenant", "topic", "tenant_and_topic"}
]

def group_pairs(items, key_name):
    out = []
    seen = sorted({item[key_name] for item in items})
    for key in seen:
        brute = next(item for item in items if item[key_name] == key and item["vector_index_mode"] == "brute_force")
        hnsw = next(item for item in items if item[key_name] == key and item["vector_index_mode"] == "hnsw_baseline")
        out.append({
            key_name: key,
            "brute_force": {"qps": brute["qps"], "p95_ms": brute["latency"]["p95_ms"]},
            "hnsw_baseline": {"qps": hnsw["qps"], "p95_ms": hnsw["latency"]["p95_ms"]},
            "delta_qps": hnsw["qps"] - brute["qps"],
            "delta_p95_ms": brute["latency"]["p95_ms"] - hnsw["latency"]["p95_ms"],
        })
    return out

summary = {
    "suite": "p8_filtered_sweep",
    "tenant_count_sweep": group_pairs(tenant_runs, "tenant_count"),
    "filter_mode_sweep": group_pairs(filter_runs, "filter_mode"),
}
summary_json.write_text(json.dumps(summary, indent=2) + "\n")

lines = ["# P8 Filtered Sweep Report", ""]
lines.append("## Tenant Count Sweep")
lines.append("")
lines.append("| Tenants | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |")
lines.append("|---:|---:|---:|---:|---:|---:|---:|")
for row in summary["tenant_count_sweep"]:
    lines.append(
        f"| {row['tenant_count']} | {row['brute_force']['qps']:.2f} | {row['hnsw_baseline']['qps']:.2f} | {row['delta_qps']:.2f} | {row['brute_force']['p95_ms']:.4f} | {row['hnsw_baseline']['p95_ms']:.4f} | {row['delta_p95_ms']:.4f} |"
    )
lines.append("")
lines.append("## Filter Mode Sweep (8 tenants)")
lines.append("")
lines.append("| Filter mode | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |")
lines.append("|---|---:|---:|---:|---:|---:|---:|")
for row in summary["filter_mode_sweep"]:
    lines.append(
        f"| {row['filter_mode']} | {row['brute_force']['qps']:.2f} | {row['hnsw_baseline']['qps']:.2f} | {row['delta_qps']:.2f} | {row['brute_force']['p95_ms']:.4f} | {row['hnsw_baseline']['p95_ms']:.4f} | {row['delta_p95_ms']:.4f} |"
    )
lines.append("")
summary_md.write_text("\n".join(lines) + "\n")
PY

echo "P8 filtered sweep suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- raw runs: $RAW_JSON"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
