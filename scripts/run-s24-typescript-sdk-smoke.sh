#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_PATH="${LOG_PATH:-project_plan/reports/s24_typescript_sdk_smoke.log}"
DIST_DIR="${DIST_DIR:-project_plan/reports/s24_typescript_dist}"

mkdir -p "$(dirname "$LOG_PATH")" "$DIST_DIR"
rm -f "$LOG_PATH"
rm -rf "$DIST_DIR"/*

echo "[typescript-sdk-install]" | tee -a "$LOG_PATH"
npm --prefix "$ROOT_DIR/sdk/typescript" install | tee -a "$LOG_PATH"

echo "[typescript-sdk-test]" | tee -a "$LOG_PATH"
npm --prefix "$ROOT_DIR/sdk/typescript" run test | tee -a "$LOG_PATH"

echo "[typescript-sdk-pack]" | tee -a "$LOG_PATH"
(cd "$ROOT_DIR/sdk/typescript" && npm pack --pack-destination "$ROOT_DIR/$DIST_DIR") | tee -a "$LOG_PATH"

echo "[s24-typescript-sdk-smoke-complete] log=${LOG_PATH}" | tee -a "$LOG_PATH"
