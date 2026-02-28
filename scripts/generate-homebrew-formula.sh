#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/packaging/homebrew/sqlrite.rb.template"
OUT="$ROOT_DIR/packaging/homebrew/sqlrite.rb"

VERSION=""
MACOS_ARM64_URL=""
MACOS_ARM64_SHA=""
MACOS_AMD64_URL=""
MACOS_AMD64_SHA=""
LINUX_ARM64_URL=""
LINUX_ARM64_SHA=""
LINUX_AMD64_URL=""
LINUX_AMD64_SHA=""

usage() {
  cat <<'USAGE'
Usage: scripts/generate-homebrew-formula.sh --version VERSION \
  --macos-arm64-url URL --macos-arm64-sha SHA \
  --macos-amd64-url URL --macos-amd64-sha SHA \
  --linux-arm64-url URL --linux-arm64-sha SHA \
  --linux-amd64-url URL --linux-amd64-sha SHA
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) shift; VERSION="$1" ;;
    --macos-arm64-url) shift; MACOS_ARM64_URL="$1" ;;
    --macos-arm64-sha) shift; MACOS_ARM64_SHA="$1" ;;
    --macos-amd64-url) shift; MACOS_AMD64_URL="$1" ;;
    --macos-amd64-sha) shift; MACOS_AMD64_SHA="$1" ;;
    --linux-arm64-url) shift; LINUX_ARM64_URL="$1" ;;
    --linux-arm64-sha) shift; LINUX_ARM64_SHA="$1" ;;
    --linux-amd64-url) shift; LINUX_AMD64_URL="$1" ;;
    --linux-amd64-sha) shift; LINUX_AMD64_SHA="$1" ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 1 ;;
  esac
  shift

done

for required in VERSION MACOS_ARM64_URL MACOS_ARM64_SHA MACOS_AMD64_URL MACOS_AMD64_SHA LINUX_ARM64_URL LINUX_ARM64_SHA LINUX_AMD64_URL LINUX_AMD64_SHA; do
  [[ -n "${!required}" ]] || { echo "missing required value: $required" >&2; usage; exit 1; }
done

sed \
  -e "s|__VERSION__|$VERSION|g" \
  -e "s|__MACOS_ARM64_URL__|$MACOS_ARM64_URL|g" \
  -e "s|__MACOS_ARM64_SHA__|$MACOS_ARM64_SHA|g" \
  -e "s|__MACOS_AMD64_URL__|$MACOS_AMD64_URL|g" \
  -e "s|__MACOS_AMD64_SHA__|$MACOS_AMD64_SHA|g" \
  -e "s|__LINUX_ARM64_URL__|$LINUX_ARM64_URL|g" \
  -e "s|__LINUX_ARM64_SHA__|$LINUX_ARM64_SHA|g" \
  -e "s|__LINUX_AMD64_URL__|$LINUX_AMD64_URL|g" \
  -e "s|__LINUX_AMD64_SHA__|$LINUX_AMD64_SHA|g" \
  "$TEMPLATE" > "$OUT"

echo "generated Homebrew formula: $OUT"
