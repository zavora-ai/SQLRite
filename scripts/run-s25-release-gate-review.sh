#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BENCH_PATH="${BENCH_PATH:-project_plan/reports/s25_benchmark_reference_integrations.json}"
CONTRACT_PATH="${CONTRACT_PATH:-project_plan/reports/s25_agent_contract_report.json}"
QUALITY_PATH="${QUALITY_PATH:-project_plan/reports/s25_quality_gates.log}"
OUTPUT_PATH="${OUTPUT_PATH:-project_plan/reports/s25_release_gate_review.md}"

python3 - <<'PY' "$BENCH_PATH" "$CONTRACT_PATH" "$QUALITY_PATH" "$OUTPUT_PATH"
import json
import pathlib
import re
import sys

bench = json.load(open(sys.argv[1], "r", encoding="utf-8"))
contract = json.load(open(sys.argv[2], "r", encoding="utf-8"))
quality_log = pathlib.Path(sys.argv[3]).read_text(encoding="utf-8")

quality_ok = "test result: ok" in quality_log
security_tests = re.findall(r"security::tests::[a-zA-Z0-9_]+ \.\.\. ok", quality_log)

lines = []
lines.append("# S25 Release Gate Review")
lines.append("")
lines.append("Generated from sprint evidence artifacts.")
lines.append("")
lines.append("## Performance")
lines.append("")
lines.append(f"- qps: `{bench['qps']:.2f}`")
lines.append(f"- p95 latency ms: `{bench['latency']['p95_ms']:.4f}`")
lines.append(f"- top1 hit rate: `{bench['top1_hit_rate']:.4f}`")
lines.append("")
lines.append("## Integration Contracts")
lines.append("")
lines.append(f"- deterministic first chunk: `{contract['deterministic_first_chunk']}`")
lines.append(f"- deterministic row count: `{contract['deterministic_row_count']}`")
lines.append(f"- setup under 15 minutes: `{contract['setup_under_15_minutes']}`")
lines.append(f"- contract gate pass: `{contract['pass']}`")
lines.append("")
lines.append("## Quality")
lines.append("")
lines.append(f"- cargo quality gates pass marker detected: `{quality_ok}`")
lines.append(f"- security test checks observed: `{len(security_tests)}`")
lines.append("")
lines.append("## Gate Outcome")
lines.append("")
pass_gate = bool(contract["pass"]) and quality_ok
lines.append(f"- overall_release_gate_pass: `{pass_gate}`")

pathlib.Path(sys.argv[4]).write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

echo "[s25-release-gate-review-complete] output=$OUTPUT_PATH"
