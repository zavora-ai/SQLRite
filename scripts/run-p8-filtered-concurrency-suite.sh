#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p8_filtered_concurrency_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p8_filtered_concurrency_suite.log"
SUMMARY_JSON="$REPORT_DIR/p8_filtered_concurrency_summary.json"
SUMMARY_MD="$REPORT_DIR/P8_filtered_concurrency.md"
RAW_JSON="$REPORT_DIR/p8_filtered_concurrency_runs.json"

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
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test benchmark_args_parse_concurrency -- --nocapture
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test filtered_chunk_ids_uses_in_memory_doc_and_metadata_index -- --nocapture
run_and_log "$QUALITY_LOG" env RUSTC_WRAPPER= cargo test hnsw_vector_search_applies_metadata_filter -- --nocapture

run_case() {
  local scenario="$1"
  local tenant_count="$2"
  local index_mode="$3"
  local concurrency="$4"
  local output_path="$5"

  run_and_log "$SUITE_LOG" env RUSTC_WRAPPER= cargo run --bin sqlrite-bench -- \
    --corpus 5000 \
    --queries 80 \
    --warmup 16 \
    --embedding-dim 64 \
    --candidate-limit 200 \
    --top-k 10 \
    --filter-mode tenant \
    --tenant-count "$tenant_count" \
    --concurrency "$concurrency" \
    --index-mode "$index_mode" \
    --storage-kind f32 \
    --output "$output_path"
}

declare -a CONCURRENCY_LEVELS=(1 2 4 8)
declare -a raw_args=()

for tenant_count in 2 8; do
  if [[ "$tenant_count" == "2" ]]; then
    scenario="low_selectivity"
  else
    scenario="high_selectivity"
  fi
  for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
    for index_mode in brute_force hnsw_baseline; do
      output_path="$REPORT_DIR/p8_filtered_concurrency_${scenario}_${index_mode}_c${concurrency}.json"
      run_case "$scenario" "$tenant_count" "$index_mode" "$concurrency" "$output_path"
      raw_args+=("$output_path")
    done
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

scenarios = []
for scenario_name, tenant_count in [("low_selectivity", 2), ("high_selectivity", 8)]:
    scenario_runs = [run for run in runs if run["tenant_count"] == tenant_count]
    grouped = []
    for concurrency in sorted({run["concurrency"] for run in scenario_runs}):
        brute = next(
            run for run in scenario_runs
            if run["concurrency"] == concurrency and run["vector_index_mode"] == "brute_force"
        )
        hnsw = next(
            run for run in scenario_runs
            if run["concurrency"] == concurrency and run["vector_index_mode"] == "hnsw_baseline"
        )
        grouped.append({
            "concurrency": concurrency,
            "brute_force": {"qps": brute["qps"], "p95_ms": brute["latency"]["p95_ms"]},
            "hnsw_baseline": {"qps": hnsw["qps"], "p95_ms": hnsw["latency"]["p95_ms"]},
            "delta_qps": hnsw["qps"] - brute["qps"],
            "delta_p95_ms": brute["latency"]["p95_ms"] - hnsw["latency"]["p95_ms"],
        })
    scenarios.append({
        "scenario": scenario_name,
        "tenant_count": tenant_count,
        "runs": grouped,
    })

summary = {
    "suite": "p8_filtered_concurrency",
    "filter_mode": "tenant",
    "scenarios": scenarios,
}
summary_json.write_text(json.dumps(summary, indent=2) + "\n")

lines = ["# P8 Filtered Concurrency Report", ""]
for scenario in scenarios:
    heading = scenario["scenario"].replace("_", " ").title()
    lines.append(f"## {heading} (`tenant_count={scenario['tenant_count']}`)")
    lines.append("")
    lines.append("| Concurrency | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |")
    lines.append("|---:|---:|---:|---:|---:|---:|---:|")
    for row in scenario["runs"]:
        lines.append(
            f"| {row['concurrency']} | {row['brute_force']['qps']:.2f} | {row['hnsw_baseline']['qps']:.2f} | {row['delta_qps']:.2f} | {row['brute_force']['p95_ms']:.4f} | {row['hnsw_baseline']['p95_ms']:.4f} | {row['delta_p95_ms']:.4f} |"
        )
    lines.append("")

summary_md.write_text("\n".join(lines) + "\n")
PY

echo "P8 filtered concurrency suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- raw runs: $RAW_JSON"
echo "- summary json: $SUMMARY_JSON"
echo "- summary markdown: $SUMMARY_MD"
