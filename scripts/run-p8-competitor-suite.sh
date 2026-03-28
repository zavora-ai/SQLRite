#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_DIR="$ROOT_DIR/project_plan/reports"
QUALITY_LOG="$REPORT_DIR/p8_competitor_quality_gates.log"
SUITE_LOG="$REPORT_DIR/p8_competitor_suite.log"
JSON_OUT="$REPORT_DIR/p8_competitor_comparison.json"
MD_OUT="$REPORT_DIR/P8_competitor_comparison.md"
P8_VENV_DIR="${SQLRITE_P8_VENV:-/tmp/sqlrite-p8-venv}"
PYTHON_BIN="${SQLRITE_P8_PYTHON:-$P8_VENV_DIR/bin/python}"
PIP_BIN="${P8_VENV_DIR}/bin/pip"

mkdir -p "$REPORT_DIR"
: > "$QUALITY_LOG"
: > "$SUITE_LOG"

run_and_log() {
  local log_file="$1"
  shift
  printf '\n$ %s\n' "$*" | tee -a "$log_file"
  "$@" 2>&1 | tee -a "$log_file"
}

ensure_competitor_python_env() {
  if [[ ! -x "$PYTHON_BIN" ]]; then
    run_and_log "$QUALITY_LOG" python3 -m venv "$P8_VENV_DIR"
  fi

  if ! "$PYTHON_BIN" -c "import sqlite_vec, lancedb" >/dev/null 2>&1; then
    run_and_log "$QUALITY_LOG" "$PIP_BIN" install --upgrade pip
    run_and_log "$QUALITY_LOG" "$PIP_BIN" install sqlite-vec lancedb
  fi
}

run_and_log "$QUALITY_LOG" cargo fmt --all --check
run_and_log "$QUALITY_LOG" cargo test benchmark_smoke_test -- --nocapture
ensure_competitor_python_env
run_and_log "$QUALITY_LOG" "$PYTHON_BIN" -m py_compile "$ROOT_DIR/scripts/run-p8-competitor-suite.py"
run_and_log "$SUITE_LOG" "$PYTHON_BIN" "$ROOT_DIR/scripts/run-p8-competitor-suite.py" --output "$JSON_OUT" --output-md "$MD_OUT"

echo "P8 competitor suite complete"
echo "- quality log: $QUALITY_LOG"
echo "- suite log: $SUITE_LOG"
echo "- json report: $JSON_OUT"
echo "- markdown report: $MD_OUT"
