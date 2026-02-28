#!/usr/bin/env bash
set -euo pipefail

REPO="${SQLRITE_REPO:-zavora-ai/SQLRite}"
VERSION="${SQLRITE_VERSION:-}"
PREFIX="${SQLRITE_INSTALL_DIR:-$HOME/.local/bin}"
SKIP_SMOKE=0

usage() {
  cat <<'USAGE'
Usage: scripts/sqlrite-install.sh --version VERSION [--prefix PATH] [--skip-smoke]

Downloads SQLRite release artifact from GitHub Releases and installs sqlrite globally.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --version" >&2; exit 1; }
      VERSION="$1"
      ;;
    --prefix)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --prefix" >&2; exit 1; }
      PREFIX="$1"
      ;;
    --skip-smoke)
      SKIP_SMOKE=1
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

[[ -n "$VERSION" ]] || { echo "--version is required" >&2; usage; exit 1; }

OS_RAW="$(uname -s)"
ARCH_RAW="$(uname -m)"
case "$OS_RAW" in
  Linux) OS="unknown-linux-gnu" ;;
  Darwin) OS="apple-darwin" ;;
  MINGW*|MSYS*|CYGWIN*) OS="pc-windows-msvc" ;;
  *) echo "unsupported OS: $OS_RAW" >&2; exit 1 ;;
esac

case "$ARCH_RAW" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "unsupported architecture: $ARCH_RAW" >&2; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
ARCHIVE="sqlrite-v${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$PREFIX"

echo "[install] downloading ${URL}"
curl -fsSL "$URL" -o "$TMP_DIR/$ARCHIVE"

echo "[install] extracting archive"
tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

BINARY_NAME="sqlrite"
if [[ "$OS" == "pc-windows-msvc" ]]; then
  BINARY_NAME="sqlrite.exe"
fi

if [[ ! -f "$TMP_DIR/$BINARY_NAME" ]]; then
  echo "binary not found in archive: $BINARY_NAME" >&2
  exit 1
fi

cp "$TMP_DIR/$BINARY_NAME" "$PREFIX/$BINARY_NAME"
chmod +x "$PREFIX/$BINARY_NAME" || true

echo "[install] installed: $PREFIX/$BINARY_NAME"

if [[ "$SKIP_SMOKE" -eq 0 ]]; then
  TMP_DB="$TMP_DIR/sqlrite-install-smoke.db"
  "$PREFIX/$BINARY_NAME" init --db "$TMP_DB" --seed-demo --profile balanced --index-mode brute_force >/dev/null
  "$PREFIX/$BINARY_NAME" doctor --db "$TMP_DB" --json >/dev/null
  echo "[install] smoke tests passed"
fi

echo "[install] ensure '$PREFIX' is in PATH"
