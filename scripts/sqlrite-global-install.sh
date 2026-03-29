#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OS_NAME="$(uname -s)"
IS_WINDOWS=0
if [[ "$OS_NAME" == MINGW* || "$OS_NAME" == MSYS* || "$OS_NAME" == CYGWIN* ]]; then
  IS_WINDOWS=1
fi

DEFAULT_PREFIX="$HOME/.local/bin"
DEFAULT_BINARY_NAME="sqlrite"
if [[ "$IS_WINDOWS" -eq 1 ]]; then
  DEFAULT_BINARY_NAME="sqlrite.exe"
fi

PREFIX="${SQLRITE_INSTALL_DIR:-$DEFAULT_PREFIX}"
BINARY_NAME="${SQLRITE_BINARY_NAME:-$DEFAULT_BINARY_NAME}"
INSTALL_PATH="${PREFIX}/${BINARY_NAME}"
MODE="copy"
SKIP_BUILD=0
SKIP_TESTS=0

usage() {
  cat <<'USAGE'
Usage: scripts/sqlrite-global-install.sh [options]

Options:
  --prefix PATH        Install directory (default: $HOME/.local/bin)
  --mode copy|symlink  Install mode (default: copy)
  --skip-build         Skip cargo build --release
  --skip-tests         Skip smoke tests after install
  --help               Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --prefix" >&2; exit 1; }
      PREFIX="$1"
      INSTALL_PATH="${PREFIX}/${BINARY_NAME}"
      ;;
    --mode)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --mode" >&2; exit 1; }
      MODE="$1"
      if [[ "$MODE" != "copy" && "$MODE" != "symlink" ]]; then
        echo "invalid --mode '$MODE' (expected copy or symlink)" >&2
        exit 1
      fi
      ;;
    --skip-build)
      SKIP_BUILD=1
      ;;
    --skip-tests)
      SKIP_TESTS=1
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

mkdir -p "$PREFIX"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "[install] building release binary..."
  (cd "$ROOT_DIR" && cargo build --release --bin sqlrite)
fi

SOURCE_BINARY="$ROOT_DIR/target/release/sqlrite"
if [[ "$IS_WINDOWS" -eq 1 ]]; then
  SOURCE_BINARY_EXE="$ROOT_DIR/target/release/sqlrite.exe"
  if [[ -f "$SOURCE_BINARY_EXE" ]]; then
    SOURCE_BINARY="$SOURCE_BINARY_EXE"
  fi
fi

if [[ ! -f "$SOURCE_BINARY" ]]; then
  echo "release binary not found at $SOURCE_BINARY" >&2
  echo "run without --skip-build or build manually first" >&2
  exit 1
fi

if [[ "$MODE" == "symlink" ]]; then
  ln -sfn "$SOURCE_BINARY" "$INSTALL_PATH"
else
  cp "$SOURCE_BINARY" "$INSTALL_PATH"
  chmod +x "$INSTALL_PATH" || true
fi

echo "[install] installed: $INSTALL_PATH"

if [[ "$SKIP_TESTS" -eq 0 ]]; then
  echo "[install] running smoke tests..."
  TMP_ROOT="${TMPDIR:-/tmp}"
  TMP_DB="${TMP_ROOT}/sqlrite-install-smoke-$$.db"
  TMP_BAK="${TMP_DB%.db}.backup.db"

  "$INSTALL_PATH" init --db "$TMP_DB" --seed-demo --profile balanced --index-mode brute_force >/dev/null
  QUERY_OUTPUT="$($INSTALL_PATH query --db "$TMP_DB" --text "local" --top-k 1)"
  if ! printf "%s" "$QUERY_OUTPUT" | grep -q "results="; then
    echo "smoke query did not return expected output" >&2
    exit 1
  fi

  "$INSTALL_PATH" doctor --db "$TMP_DB" --json >/dev/null
  "$INSTALL_PATH" backup --source "$TMP_DB" --dest "$TMP_BAK" >/dev/null
  "$INSTALL_PATH" backup verify --path "$TMP_BAK" >/dev/null

  rm -f "$TMP_DB" "$TMP_BAK"
  echo "[install] smoke tests passed"
fi

PATH_SEP=':'
if [[ "$IS_WINDOWS" -eq 1 ]]; then
  PATH_SEP=';'
fi

if printf '%s' "$PATH" | tr "$PATH_SEP" '\n' | grep -Fxq "$PREFIX"; then
  :
else
  echo "[install] note: '$PREFIX' is not in PATH"
  echo "[install] add this to your shell config: export PATH=\"$PREFIX:\$PATH\""
fi

if command -v "$BINARY_NAME" >/dev/null 2>&1; then
  echo "[install] active '$BINARY_NAME' path: $(command -v "$BINARY_NAME")"
else
  echo "[install] run with full path until PATH is updated: $INSTALL_PATH"
fi
