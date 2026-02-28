#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE_DIR="$ROOT_DIR/packaging/winget/templates"

VERSION=""
WIN_AMD64_URL=""
WIN_AMD64_SHA=""
WIN_ARM64_URL=""
WIN_ARM64_SHA=""

usage() {
  cat <<'USAGE'
Usage: scripts/generate-winget-manifests.sh --version VERSION \
  --windows-amd64-url URL --windows-amd64-sha SHA \
  --windows-arm64-url URL --windows-arm64-sha SHA
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) shift; VERSION="$1" ;;
    --windows-amd64-url) shift; WIN_AMD64_URL="$1" ;;
    --windows-amd64-sha) shift; WIN_AMD64_SHA="$1" ;;
    --windows-arm64-url) shift; WIN_ARM64_URL="$1" ;;
    --windows-arm64-sha) shift; WIN_ARM64_SHA="$1" ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 1 ;;
  esac
  shift

done

for required in VERSION WIN_AMD64_URL WIN_AMD64_SHA WIN_ARM64_URL WIN_ARM64_SHA; do
  [[ -n "${!required}" ]] || { echo "missing required value: $required" >&2; usage; exit 1; }
done

OUT_DIR="$ROOT_DIR/packaging/winget/manifests/z/zavora-ai/sqlrite/$VERSION"
mkdir -p "$OUT_DIR"

render() {
  local src="$1"
  local dst="$2"
  sed \
    -e "s|__VERSION__|$VERSION|g" \
    -e "s|__WINDOWS_AMD64_URL__|$WIN_AMD64_URL|g" \
    -e "s|__WINDOWS_AMD64_SHA__|$WIN_AMD64_SHA|g" \
    -e "s|__WINDOWS_ARM64_URL__|$WIN_ARM64_URL|g" \
    -e "s|__WINDOWS_ARM64_SHA__|$WIN_ARM64_SHA|g" \
    "$src" > "$dst"
}

render "$TEMPLATE_DIR/zavora-ai.sqlrite.yaml.template" "$OUT_DIR/zavora-ai.sqlrite.yaml"
render "$TEMPLATE_DIR/zavora-ai.sqlrite.locale.en-US.yaml.template" "$OUT_DIR/zavora-ai.sqlrite.locale.en-US.yaml"
render "$TEMPLATE_DIR/zavora-ai.sqlrite.installer.yaml.template" "$OUT_DIR/zavora-ai.sqlrite.installer.yaml"

echo "generated winget manifests: $OUT_DIR"
