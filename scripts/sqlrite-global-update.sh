#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="$ROOT_DIR/scripts/sqlrite-global-install.sh"
RUN_FULL_GATES=1
INSTALL_ARGS=()

usage() {
  cat <<'USAGE'
Usage: scripts/sqlrite-global-update.sh [options]

Options:
  --quick              Skip full quality gates (fmt/clippy/test)
  --prefix PATH        Pass custom install directory to installer
  --mode copy|symlink  Pass install mode to installer
  --help               Show this help

Behavior:
  Default mode runs:
    1) cargo fmt --all --check
    2) cargo clippy --all-targets --all-features -- -D warnings
    3) cargo test
    4) scripts/sqlrite-global-install.sh (with smoke tests)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick)
      RUN_FULL_GATES=0
      ;;
    --prefix|--mode)
      INSTALL_ARGS+=("$1")
      shift
      [[ $# -gt 0 ]] || { echo "missing value for ${INSTALL_ARGS[-1]}" >&2; exit 1; }
      INSTALL_ARGS+=("$1")
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
  shift

done

if [[ "$RUN_FULL_GATES" -eq 1 ]]; then
  echo "[update] running quality gates..."
  (cd "$ROOT_DIR" && cargo fmt --all --check)
  (cd "$ROOT_DIR" && cargo clippy --all-targets --all-features -- -D warnings)
  (cd "$ROOT_DIR" && cargo test)
  echo "[update] quality gates passed"
else
  echo "[update] quick mode: skipping fmt/clippy/test"
fi

echo "[update] rebuilding/reinstalling global sqlrite with smoke tests..."
if [[ ${#INSTALL_ARGS[@]} -gt 0 ]]; then
  "$INSTALL_SCRIPT" "${INSTALL_ARGS[@]}"
else
  "$INSTALL_SCRIPT"
fi

echo "[update] global sqlrite update complete"
