#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p8_competitor_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p8_competitor_suite.log"
JSON_OUT="$REPORT_DIR/p8_competitor_comparison.json"
MD_OUT="$REPORT_DIR/P8_competitor_comparison.md"

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
run_and_log "$QUALITY_LOG" cargo test benchmark_smoke_test -- --nocapture
run_and_log "$QUALITY_LOG" python3 -m py_compile "$ROOT_DIR/scripts/run-p8-competitor-suite.py"
run_and_log "$SUITE_LOG" python3 "$ROOT_DIR/scripts/run-p8-competitor-suite.py" --output "$JSON_OUT" --output-md "$MD_OUT"

echo "P8 competitor suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- json report: $JSON_OUT"
echo "- markdown report: $MD_OUT"
