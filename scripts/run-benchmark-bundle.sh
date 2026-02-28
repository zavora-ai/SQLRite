#!/usr/bin/env bash

set -euo pipefail

OUTPUT_DIR="project_plan/reports/s13_bundle"
PROFILES="100k,1m"
CONCURRENCY_PROFILE="100k"
CONCURRENCY_LEVELS="1,2,4"
DATASET_PATH="examples/eval_dataset.json"
DATASET_ID="examples/eval_dataset.json"
EMBEDDING_MODEL="deterministic-local-v1"
HARDWARE_CLASS="local-$(uname -s)-$(uname -m)"
DURABILITY="balanced"
SKIP_EVAL=0
SKIP_STRICT_GATE=1
ALLOW_GATE_FAIL=1

usage() {
  cat <<'USAGE'
usage: bash scripts/run-benchmark-bundle.sh [options]

options:
  --output-dir PATH           Output directory (default: project_plan/reports/s13_bundle)
  --profiles CSV              Benchmark profiles CSV (default: 100k,1m)
  --with-10m                  Append 10m profile to --profiles
  --concurrency-profile NAME  Sweep profile (default: 100k)
  --concurrency-levels CSV    Sweep levels CSV (default: 1,2,4)
  --dataset PATH              Eval dataset path (default: examples/eval_dataset.json)
  --dataset-id ID             Dataset identifier label
  --embedding-model NAME      Embedding model label
  --hardware-class NAME       Hardware class label
  --durability NAME           balanced|durable|fast_unsafe (default: balanced)
  --skip-eval                 Skip eval pass in suite run
  --strict-phase-c-gate       Enforce Phase C target gates (PC-G01/PC-G02/PC-G03)
  --disallow-gate-fail        Fail script if strict gate fails (default allows fail)
  --help                      Show help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      OUTPUT_DIR="${2:-}"
      shift 2
      ;;
    --profiles)
      PROFILES="${2:-}"
      shift 2
      ;;
    --with-10m)
      if [[ ",${PROFILES}," != *",10m,"* ]]; then
        PROFILES="${PROFILES},10m"
      fi
      shift
      ;;
    --concurrency-profile)
      CONCURRENCY_PROFILE="${2:-}"
      shift 2
      ;;
    --concurrency-levels)
      CONCURRENCY_LEVELS="${2:-}"
      shift 2
      ;;
    --dataset)
      DATASET_PATH="${2:-}"
      shift 2
      ;;
    --dataset-id)
      DATASET_ID="${2:-}"
      shift 2
      ;;
    --embedding-model)
      EMBEDDING_MODEL="${2:-}"
      shift 2
      ;;
    --hardware-class)
      HARDWARE_CLASS="${2:-}"
      shift 2
      ;;
    --durability)
      DURABILITY="${2:-}"
      shift 2
      ;;
    --skip-eval)
      SKIP_EVAL=1
      shift
      ;;
    --strict-phase-c-gate)
      SKIP_STRICT_GATE=0
      ALLOW_GATE_FAIL=1
      shift
      ;;
    --disallow-gate-fail)
      ALLOW_GATE_FAIL=0
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1"
      usage
      exit 1
      ;;
  esac
done

mkdir -p "$OUTPUT_DIR"

SUITE_JSON="$OUTPUT_DIR/bench_suite.json"
SUITE_LOG="$OUTPUT_DIR/bench_suite.log"
GATE_LOG="$OUTPUT_DIR/phase_c_gate.log"
BUNDLE_TAR="$OUTPUT_DIR/benchmark_bundle.tar.gz"
MANIFEST_JSON="$OUTPUT_DIR/manifest.json"

rm -f "$SUITE_JSON" "$SUITE_LOG" "$GATE_LOG" "$BUNDLE_TAR"

RUN_CMD=(
  cargo run --bin sqlrite-bench-suite --
  --profiles "$PROFILES"
  --concurrency-profile "$CONCURRENCY_PROFILE"
  --concurrency-levels "$CONCURRENCY_LEVELS"
  --dataset "$DATASET_PATH"
  --dataset-id "$DATASET_ID"
  --embedding-model "$EMBEDDING_MODEL"
  --hardware-class "$HARDWARE_CLASS"
  --durability "$DURABILITY"
  --output "$SUITE_JSON"
)

if [[ "$SKIP_EVAL" -eq 1 ]]; then
  RUN_CMD+=(--skip-eval)
fi

echo "[bundle] running benchmark suite"
echo "[bundle] command: ${RUN_CMD[*]}"
"${RUN_CMD[@]}" | tee "$SUITE_LOG"

STRICT_GATE_STATUS="skipped"
if [[ "$SKIP_STRICT_GATE" -eq 0 ]]; then
  echo "[bundle] running strict phase-c gate assertions"
  set +e
  cargo run --bin sqlrite-bench-suite-assert -- \
    --suite "$SUITE_JSON" \
    --rule "profile=100k,scenario=weighted + lsh_ann,max_p95_ms=40,min_top1=0.99" \
    --rule "profile=1m,scenario=weighted + lsh_ann,max_p95_ms=90,min_top1=0.99" \
    --rule "profile=100k,scenario=weighted + brute_force,min_ingest_cpm=50000" \
    --eval-rule "index_mode=brute_force,min_recall_k1=0.80,min_mrr_k1=0.95,min_ndcg_k1=0.95" \
    --eval-rule "index_mode=lsh_ann,min_recall_k1=0.80,min_mrr_k1=0.95,min_ndcg_k1=0.95" \
    --eval-rule "index_mode=hnsw_baseline,min_recall_k1=0.80,min_mrr_k1=0.95,min_ndcg_k1=0.95" \
    >"$GATE_LOG" 2>&1
  GATE_EXIT=$?
  set -e
  if [[ $GATE_EXIT -eq 0 ]]; then
    STRICT_GATE_STATUS="passed"
  else
    STRICT_GATE_STATUS="failed"
    echo "[bundle] strict phase-c gate failed; see $GATE_LOG"
    if [[ "$ALLOW_GATE_FAIL" -eq 0 ]]; then
      cat "$GATE_LOG"
      exit $GATE_EXIT
    fi
  fi
else
  echo "[bundle] strict phase-c gate skipped"
  echo "strict phase-c gate skipped" >"$GATE_LOG"
fi

cat >"$MANIFEST_JSON" <<MANIFEST
{
  "generated_at_unix_seconds": $(date +%s),
  "profiles": "$PROFILES",
  "concurrency_profile": "$CONCURRENCY_PROFILE",
  "concurrency_levels": "$CONCURRENCY_LEVELS",
  "dataset_path": "$DATASET_PATH",
  "dataset_id": "$DATASET_ID",
  "embedding_model": "$EMBEDDING_MODEL",
  "hardware_class": "$HARDWARE_CLASS",
  "durability": "$DURABILITY",
  "skip_eval": $([[ "$SKIP_EVAL" -eq 1 ]] && echo "true" || echo "false"),
  "strict_phase_c_gate_status": "$STRICT_GATE_STATUS",
  "artifacts": {
    "suite_json": "$(basename "$SUITE_JSON")",
    "suite_log": "$(basename "$SUITE_LOG")",
    "gate_log": "$(basename "$GATE_LOG")"
  }
}
MANIFEST

(
  cd "$OUTPUT_DIR"
  tar -czf "$(basename "$BUNDLE_TAR")" \
    "$(basename "$SUITE_JSON")" \
    "$(basename "$SUITE_LOG")" \
    "$(basename "$MANIFEST_JSON")" \
    "$(basename "$GATE_LOG")"
)

echo "[bundle] artifacts:"
echo "- $SUITE_JSON"
echo "- $SUITE_LOG"
echo "- $GATE_LOG"
echo "- $MANIFEST_JSON"
echo "- $BUNDLE_TAR"
