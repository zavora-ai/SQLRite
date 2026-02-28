#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=""

usage() {
  cat <<'USAGE'
Usage: scripts/package-linux.sh --version VERSION

Outputs in dist/:
- tar.gz archive for host Linux target
- .deb and .rpm if nfpm is installed
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      shift
      [[ $# -gt 0 ]] || { echo "missing value for --version" >&2; exit 1; }
      VERSION="$1"
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

TARGET="$(rustc -vV | awk -F': ' '/host:/ {print $2}')"

"$ROOT_DIR/scripts/create-release-archive.sh" --version "$VERSION" --target "$TARGET"

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) NFP_ARCH="amd64" ;;
  aarch64|arm64) NFP_ARCH="arm64" ;;
  *)
    echo "unsupported arch for nfpm packaging: $ARCH" >&2
    exit 1
    ;;
esac

if ! command -v nfpm >/dev/null 2>&1; then
  echo "nfpm not found; skipping deb/rpm packaging"
  exit 0
fi

mkdir -p "$ROOT_DIR/dist"

(
  cd "$ROOT_DIR"
  VERSION="$VERSION" ARCH="$NFP_ARCH" nfpm package -f packaging/nfpm/nfpm.yaml -p deb --target "dist/sqlrite-v${VERSION}-linux-${NFP_ARCH}.deb"
  VERSION="$VERSION" ARCH="$NFP_ARCH" nfpm package -f packaging/nfpm/nfpm.yaml -p rpm --target "dist/sqlrite-v${VERSION}-linux-${NFP_ARCH}.rpm"
)

echo "created Linux packages in dist/"
