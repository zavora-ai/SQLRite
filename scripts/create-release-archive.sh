#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
VERSION=""
TARGET=""

usage() {
  cat <<'USAGE'
Usage: scripts/create-release-archive.sh --version VERSION [--target TARGET]

Builds release sqlrite binary and creates:
- dist/sqlrite-v<VERSION>-<TARGET>.tar.gz
- dist/sqlrite-v<VERSION>-<TARGET>.sha256
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --version" >&2; exit 1; }
      VERSION="$1"
      ;;
    --target)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --target" >&2; exit 1; }
      TARGET="$1"
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

if [[ -z "$TARGET" ]]; then
  TARGET="$(rustc -vV | awk -F': ' '/host:/ {print $2}')"
fi

mkdir -p "$DIST_DIR"

(cd "$ROOT_DIR" && cargo build --release --bin sqlrite)

BIN_NAME="sqlrite"
if [[ "$TARGET" == *"windows"* ]]; then
  BIN_NAME="sqlrite.exe"
fi

SOURCE_BIN="$ROOT_DIR/target/release/$BIN_NAME"
[[ -f "$SOURCE_BIN" ]] || { echo "binary not found: $SOURCE_BIN" >&2; exit 1; }

STAGE_DIR="$DIST_DIR/sqlrite-$TARGET"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"
cp "$SOURCE_BIN" "$STAGE_DIR/"

ARCHIVE_BASE="sqlrite-v${VERSION}-${TARGET}"
ARCHIVE_PATH="$DIST_DIR/${ARCHIVE_BASE}.tar.gz"

(
  cd "$STAGE_DIR"
  tar -czf "$ARCHIVE_PATH" "$BIN_NAME"
)

SHA_PATH="$DIST_DIR/${ARCHIVE_BASE}.sha256"
shasum -a 256 "$ARCHIVE_PATH" > "$SHA_PATH"

echo "created archive: $ARCHIVE_PATH"
echo "created sha256: $SHA_PATH"
