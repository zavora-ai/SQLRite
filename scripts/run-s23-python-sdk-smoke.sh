#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_PATH="${LOG_PATH:-project_plan/reports/s23_python_sdk_smoke.log}"
DIST_DIR="${DIST_DIR:-project_plan/reports/s23_python_dist}"

if [[ -z "${VENV_DIR:-}" ]]; then
  VENV_DIR="$(mktemp -d "${TMPDIR:-/tmp}/sqlrite-s23-venv.XXXXXX")"
  CLEANUP_VENV=1
else
  rm -rf "${VENV_DIR}"
  CLEANUP_VENV=0
fi

cleanup() {
  if [[ "$CLEANUP_VENV" == "1" ]]; then
    rm -rf "$VENV_DIR"
  fi
}
trap cleanup EXIT

mkdir -p "$(dirname "$LOG_PATH")" "$DIST_DIR"
rm -f "$LOG_PATH"
rm -rf "$DIST_DIR"/*

echo "[python-sdk-tests]" | tee -a "$LOG_PATH"
PYTHONPATH="$ROOT_DIR/sdk/python" python3 -m unittest -v sdk/python/tests/test_client.py \
  | tee -a "$LOG_PATH"

echo "[python-sdk-build]" | tee -a "$LOG_PATH"
python3 -m venv "$VENV_DIR"
"$VENV_DIR/bin/python" -m pip install --quiet --upgrade pip build >/dev/null
"$VENV_DIR/bin/python" -m build "$ROOT_DIR/sdk/python" --outdir "$DIST_DIR" | tee -a "$LOG_PATH"

echo "[s23-python-sdk-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
