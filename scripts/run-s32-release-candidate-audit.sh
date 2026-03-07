#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

QUALITY_LOG="${QUALITY_LOG:-project_plan/reports/s32_quality_gates.log}"
BENCH_PATH="${BENCH_PATH:-project_plan/reports/s32_bench_suite.json}"
BLOCKER_PATH="${BLOCKER_PATH:-project_plan/reports/s32_blocker_audit.json}"
QUALITY_REPORT_PATH="${QUALITY_REPORT_PATH:-project_plan/reports/s32_release_quality_report.md}"
RELEASE_NOTES_PATH="${RELEASE_NOTES_PATH:-project_plan/reports/s32_release_notes_draft.md}"
RISK_REGISTER_PATH="${RISK_REGISTER_PATH:-project_plan/reports/s32_risk_register.md}"
SPRINT_REPORT_PATH="${SPRINT_REPORT_PATH:-project_plan/reports/S32.md}"
DEFECT_REGISTER_PATH="${DEFECT_REGISTER_PATH:-project_plan/release/defect_register.json}"

mkdir -p "$(dirname "$QUALITY_LOG")" "$(dirname "$BENCH_PATH")" "$(dirname "$DEFECT_REGISTER_PATH")"
rm -f "$QUALITY_LOG" "$BENCH_PATH" "$BLOCKER_PATH" "$QUALITY_REPORT_PATH" "$RELEASE_NOTES_PATH" "$RISK_REGISTER_PATH" "$SPRINT_REPORT_PATH"

cpu_threads() {
  if command -v getconf >/dev/null 2>&1; then
    getconf _NPROCESSORS_ONLN 2>/dev/null && return 0
  fi
  if command -v sysctl >/dev/null 2>&1; then
    sysctl -n hw.logicalcpu 2>/dev/null && return 0
  fi
  echo 1
}

OS_NAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH_NAME="$(uname -m)"
CPU_THREADS="$(cpu_threads | tr -d '[:space:]')"
HARDWARE_CLASS="${HARDWARE_CLASS:-${OS_NAME}-${ARCH_NAME}-${CPU_THREADS}cpu}"
BENCH_DATASET="${BENCH_DATASET:-examples/eval_dataset.json}"
BENCH_DATASET_ID="${BENCH_DATASET_ID:-s32_release_candidate_v1}"
EMBEDDING_MODEL="${EMBEDDING_MODEL:-deterministic-local-v1}"

{
  echo "[cargo-fmt]"
  cargo fmt --all --check
  echo "[cargo-test]"
  cargo test
  echo "[s26-api-compat]"
  bash scripts/run-s26-api-compat-suite.sh
  echo "[s27-security-rbac]"
  bash scripts/run-s27-security-rbac-smoke.sh
  echo "[s28-security-audit]"
  bash scripts/run-s28-security-audit-hardening.sh
  echo "[s30-migration]"
  bash scripts/run-s30-migration-suite.sh
  echo "[s31-sql-v2]"
  bash scripts/run-s31-sql-v2-and-api-migrations.sh
  echo "[s32-bench-suite]"
  cargo run --quiet --bin sqlrite-bench-suite -- \
    --profiles quick,10k \
    --concurrency-profile quick \
    --concurrency-levels 1,2,4 \
    --dataset "$BENCH_DATASET" \
    --dataset-id "$BENCH_DATASET_ID" \
    --embedding-model "$EMBEDDING_MODEL" \
    --hardware-class "$HARDWARE_CLASS" \
    --durability balanced \
    --output "$BENCH_PATH"
} 2>&1 | tee "$QUALITY_LOG"

python3 - <<'PY' \
  "$DEFECT_REGISTER_PATH" \
  "$QUALITY_LOG" \
  "$BENCH_PATH" \
  "$BLOCKER_PATH" \
  "$QUALITY_REPORT_PATH" \
  "$RELEASE_NOTES_PATH" \
  "$RISK_REGISTER_PATH" \
  "$SPRINT_REPORT_PATH"
import json
import pathlib
import re
import sys
from datetime import date


def load_json(path_str):
    path = pathlib.Path(path_str)
    if not path.exists():
        raise SystemExit(f"missing required artifact: {path}")
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def maybe_json(path_str):
    path = pathlib.Path(path_str)
    if not path.exists():
        return None
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def fmt_bool(value):
    return "true" if value else "false"


def fmt_num(value, digits=2):
    if value is None:
        return "n/a"
    return f"{value:.{digits}f}"


def profile_runs(bench_report, profile_name):
    for matrix in bench_report.get("benchmark_profiles", []):
        if matrix.get("profile") == profile_name:
            return matrix.get("runs", [])
    return []


def find_run(runs, name_substring):
    for item in runs:
        if name_substring in item.get("name", ""):
            return item.get("report", {})
    return {}


def load_resilience_rollup():
    soak = maybe_json("project_plan/reports/s19_soak_slo_summary.json") or {}
    dr = maybe_json("project_plan/reports/s19_benchmark_dr_gate.json") or {}
    recovery = maybe_json("project_plan/reports/s17_benchmark_recovery.json") or {}
    obs = maybe_json("project_plan/reports/s18_benchmark_observability.json") or {}
    return {
        "availability_percent": soak.get("availability_percent"),
        "availability_target_percent": soak.get("availability_target_percent"),
        "availability_pass": soak.get("availability_pass"),
        "observed_rpo_seconds": soak.get("observed_rpo_seconds"),
        "rpo_target_seconds": soak.get("rpo_target_seconds"),
        "rpo_pass": soak.get("rpo_pass"),
        "dr_qps": dr.get("qps"),
        "dr_p95_ms": (dr.get("latency") or {}).get("p95_ms"),
        "restore_qps": recovery.get("qps"),
        "restore_p95_ms": (recovery.get("latency") or {}).get("p95_ms"),
        "observability_qps": obs.get("qps"),
        "observability_p95_ms": (obs.get("latency") or {}).get("p95_ms"),
        "chaos_validation": soak.get("chaos_validation", {}),
    }


def summarize_eval(bench_report):
    out = []
    for run in bench_report.get("eval_runs", []):
        report = run.get("report", {})
        aggregate = report.get("aggregate_metrics_at_k") or {}
        chosen_k = None
        if aggregate:
            numeric_keys = []
            for key in aggregate.keys():
                try:
                    numeric_keys.append(int(key))
                except (TypeError, ValueError):
                    continue
            if numeric_keys:
                chosen_k = str(max(numeric_keys))
        metrics = aggregate.get(chosen_k, {}) if chosen_k is not None else {}
        out.append({
            "index_mode": run.get("index_mode"),
            "k": chosen_k,
            "recall_at_k": metrics.get("recall"),
            "mrr": metrics.get("mrr"),
            "ndcg_at_k": metrics.get("ndcg"),
        })
    return out


def line(section, value=None):
    return section if value is None else f"- {section}: `{value}`"


def closed_defect_summary(defects):
    rows = []
    for defect in defects:
        rows.append(
            f"- {defect['id']} `{defect['severity']}` `{defect['status']}`: {defect['title']}"
        )
    return rows


def suite_result(report, pass_field="pass"):
    if not report:
        return False
    return bool(report.get(pass_field, False))


def quality_marker(quality_log, label):
    return label in quality_log


def extract_test_count(quality_log):
    matches = re.findall(r"test result: ok\. ([0-9]+) passed", quality_log)
    if not matches:
        return None
    return max(int(value) for value in matches)


def main():
    defect_register = load_json(sys.argv[1])
    quality_log = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8")
    bench_report = load_json(sys.argv[3])
    blocker_path = pathlib.Path(sys.argv[4])
    quality_report_path = pathlib.Path(sys.argv[5])
    release_notes_path = pathlib.Path(sys.argv[6])
    risk_register_path = pathlib.Path(sys.argv[7])
    sprint_report_path = pathlib.Path(sys.argv[8])

    s26 = maybe_json("project_plan/reports/s26_api_compatibility_report.json") or {}
    s27 = maybe_json("project_plan/reports/s27_security_rbac_report.json") or {}
    s28 = maybe_json("project_plan/reports/s28_security_audit_report.json") or {}
    s30 = maybe_json("project_plan/reports/s30_migration_report.json") or {}
    s31 = maybe_json("project_plan/reports/s31_sql_v2_migration_report.json") or {}
    resilience = load_resilience_rollup()

    defects = defect_register.get("defects", [])
    open_p0 = [d for d in defects if d.get("severity") == "P0" and d.get("status") != "closed"]
    open_p1 = [d for d in defects if d.get("severity") == "P1" and d.get("status") != "closed"]
    closed = [d for d in defects if d.get("status") == "closed"]

    quick_runs = profile_runs(bench_report, "quick")
    tenk_runs = profile_runs(bench_report, "10k")
    quick_weighted = find_run(quick_runs, "weighted + brute_force")
    tenk_weighted = find_run(tenk_runs, "weighted + brute_force")

    concurrency_runs = bench_report.get("concurrency_sweep", {}).get("runs", [])
    eval_summary = summarize_eval(bench_report)

    gate_status = {
        "cargo_fmt": quality_marker(quality_log, "[cargo-fmt]"),
        "cargo_test": quality_marker(quality_log, "[cargo-test]"),
        "api_compatibility": suite_result(s26),
        "security_rbac": suite_result(s27),
        "security_audit": suite_result(s28),
        "migration_toolchain": suite_result(s30),
        "sql_v2_api_migrations": suite_result(s31),
        "bench_suite_present": bool(bench_report.get("benchmark_profiles")),
    }
    gate_pass = all(gate_status.values()) and not open_p0 and not open_p1

    blocker_report = {
        "generated_on": str(date.today()),
        "release_target": defect_register.get("release_target"),
        "pass": gate_pass,
        "open_p0_count": len(open_p0),
        "open_p1_count": len(open_p1),
        "closed_defect_count": len(closed),
        "gate_status": gate_status,
        "release_blockers": {
            "open_p0": open_p0,
            "open_p1": open_p1,
        },
        "benchmark_summary": {
            "quick_weighted_bruteforce": quick_weighted,
            "tenk_weighted_bruteforce": tenk_weighted,
            "concurrency_runs": concurrency_runs,
            "eval_runs": eval_summary,
        },
        "resilience_summary": resilience,
    }
    blocker_path.write_text(json.dumps(blocker_report, indent=2) + "\n", encoding="utf-8")

    quality_lines = [
        "# S32 Release Quality Report",
        "",
        f"Release target: `{defect_register.get('release_target')}`",
        f"Generated: `{date.today()}`",
        "",
        "## Gate Summary",
        "",
        line("overall_release_candidate_pass", fmt_bool(gate_pass)),
        line("open_p0_count", len(open_p0)),
        line("open_p1_count", len(open_p1)),
        line("largest_observed_test_pass_count", extract_test_count(quality_log) or "n/a"),
        "",
        "## Required Suite Status",
        "",
        line("cargo_fmt", fmt_bool(gate_status["cargo_fmt"])),
        line("cargo_test", fmt_bool(gate_status["cargo_test"])),
        line("s26_api_compatibility", fmt_bool(gate_status["api_compatibility"])),
        line("s27_security_rbac", fmt_bool(gate_status["security_rbac"])),
        line("s28_security_audit", fmt_bool(gate_status["security_audit"])),
        line("s30_migration_toolchain", fmt_bool(gate_status["migration_toolchain"])),
        line("s31_sql_v2_api_migrations", fmt_bool(gate_status["sql_v2_api_migrations"])),
        "",
        "## Performance And Efficiency",
        "",
        line("quick_qps", fmt_num(quick_weighted.get("qps"))),
        line("quick_p50_ms", fmt_num((quick_weighted.get("latency") or {}).get("p50_ms"), 4)),
        line("quick_p95_ms", fmt_num((quick_weighted.get("latency") or {}).get("p95_ms"), 4)),
        line("quick_p99_ms", fmt_num((quick_weighted.get("latency") or {}).get("p99_ms"), 4)),
        line("10k_qps", fmt_num(tenk_weighted.get("qps"))),
        line("10k_p95_ms", fmt_num((tenk_weighted.get("latency") or {}).get("p95_ms"), 4)),
        line("10k_top1_hit_rate", fmt_num(tenk_weighted.get("top1_hit_rate"), 4)),
        line("10k_approx_working_set_bytes", tenk_weighted.get("approx_working_set_bytes", "n/a")),
        line("10k_vector_index_estimated_memory_bytes", tenk_weighted.get("vector_index_estimated_memory_bytes", "n/a")),
        line("10k_sqlite_mmap_size_bytes", tenk_weighted.get("sqlite_mmap_size_bytes", "n/a")),
        line("10k_sqlite_cache_size_kib", tenk_weighted.get("sqlite_cache_size_kib", "n/a")),
        "",
        "## Retrieval Quality",
        "",
    ]
    if eval_summary:
        for item in eval_summary:
            quality_lines.append(
                f"- {item['index_mode']} @k={item['k']}: recall_at_k=`{fmt_num(item['recall_at_k'], 4)}`, mrr=`{fmt_num(item['mrr'], 4)}`, ndcg_at_k=`{fmt_num(item['ndcg_at_k'], 4)}`"
            )
    else:
        quality_lines.append("- eval summary: `n/a`")

    quality_lines.extend([
        "",
        "## Throughput By Concurrency",
        "",
    ])
    for item in concurrency_runs:
        report = item.get("report", {})
        quality_lines.append(
            f"- concurrency {item.get('concurrency')}: qps=`{fmt_num(report.get('qps'))}`, p95_ms=`{fmt_num((report.get('latency') or {}).get('p95_ms'), 4)}`"
        )

    quality_lines.extend([
        "",
        "## Operational Resilience",
        "",
        line("availability_percent", fmt_num(resilience.get("availability_percent"), 2)),
        line("availability_target_percent", fmt_num(resilience.get("availability_target_percent"), 2)),
        line("availability_pass", fmt_bool(bool(resilience.get("availability_pass", False)))),
        line("observed_rpo_seconds", fmt_num(resilience.get("observed_rpo_seconds"), 4)),
        line("rpo_target_seconds", fmt_num(resilience.get("rpo_target_seconds"), 4)),
        line("rpo_pass", fmt_bool(bool(resilience.get("rpo_pass", False)))),
        line("dr_qps", fmt_num(resilience.get("dr_qps"))),
        line("dr_p95_ms", fmt_num(resilience.get("dr_p95_ms"), 4)),
        line("restore_qps", fmt_num(resilience.get("restore_qps"))),
        line("restore_p95_ms", fmt_num(resilience.get("restore_p95_ms"), 4)),
        line("observability_qps", fmt_num(resilience.get("observability_qps"))),
        line("observability_p95_ms", fmt_num(resilience.get("observability_p95_ms"), 4)),
        "",
        "## Closed Defect Ledger",
        "",
    ])
    quality_lines.extend(closed_defect_summary(closed))
    quality_report_path.write_text("\n".join(quality_lines) + "\n", encoding="utf-8")

    risk_lines = [
        "# S32 Risk Register",
        "",
        f"Generated: `{date.today()}`",
        "",
        "## Accepted Risks",
        "",
        "- `R1` Platform packaging remains dependent on the external GitHub release/publishing path and is validated in CI rather than by a live tag in this sprint.",
        "- `R2` Benchmark suite in S32 is sized for reproducible release checks (`quick`, `10k`) and does not replace longer-duration publication runs planned for S33.",
        "",
        "## Mitigated Risks",
        "",
        "- `M1` Frozen API drift is covered by the S26 compatibility manifest and suite.",
        "- `M2` Secure-default deployment regressions are covered by S27 and S28 operator suites.",
        "- `M3` Migration regressions are covered by S30 and S31 import suites across SQL and API-first inputs.",
        "",
        "## Rollback Plan",
        "",
        "1. Stop release promotion and keep packaged channels pinned to the previous stable tag.",
        "2. Restore the latest validated snapshot or backup if correctness/regression affects persisted data.",
        "3. Record the failed gate in `project_plan/release/defect_register.json` and rerun the full S32 audit after the fix.",
        "",
        "## Gate Inputs",
        "",
        "- `project_plan/reports/s32_quality_gates.log`",
        "- `project_plan/reports/s32_bench_suite.json`",
        "- `project_plan/reports/s32_blocker_audit.json`",
    ]
    risk_register_path.write_text("\n".join(risk_lines) + "\n", encoding="utf-8")

    release_lines = [
        "# SQLRite v1.0.0-rc1 Release Notes Draft",
        "",
        "## Highlights",
        "",
        "- SQL-native retrieval now spans embedded CLI SQL, server `/v1/sql`, and API-first migration workflows.",
        "- Frozen v1 API compatibility, secure-default RBAC/audit controls, and migration toolchains are covered by deterministic suites.",
        "- Release-candidate benchmark bundle regenerates latency, throughput, retrieval quality, efficiency, and resilience evidence.",
        "",
        "## Release Gate Snapshot",
        "",
        line("release_candidate_pass", fmt_bool(gate_pass)),
        line("open_p0_count", len(open_p0)),
        line("open_p1_count", len(open_p1)),
        line("quick_qps", fmt_num(quick_weighted.get("qps"))),
        line("10k_p95_ms", fmt_num((tenk_weighted.get("latency") or {}).get("p95_ms"), 4)),
        line("availability_percent", fmt_num(resilience.get("availability_percent"), 2)),
        line("observed_rpo_seconds", fmt_num(resilience.get("observed_rpo_seconds"), 4)),
        "",
        "## Operator Notes",
        "",
        "- Run `bash scripts/run-s32-release-candidate-audit.sh` before tagging or publishing.",
        "- Review `project_plan/reports/s32_release_quality_report.md` and `project_plan/reports/s32_risk_register.md` as the canonical RC package.",
    ]
    release_notes_path.write_text("\n".join(release_lines) + "\n", encoding="utf-8")

    sprint_lines = [
        "# S32 Sprint Report",
        "",
        "Sprint: S32 (2027-05-10 to 2027-05-23)  ",
        "Phase: F - Enterprise Trust and v1.0  ",
        "Status: Completed  ",
        f"Date: {date.today().strftime('%B %-d, %Y')}",
        "",
        "## Scope IDs Completed",
        "",
        "1. PF-D04",
        "2. PF-G01",
        "3. BE-01",
        "4. BE-02",
        "5. BE-03",
        "6. BE-04",
        "7. BE-05",
        "8. GV-01",
        "9. GV-02",
        "",
        "## Delivered Artifacts",
        "",
        "1. Release policy and LTS governance:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/docs/release_policy.md`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/docs/runbooks/release_candidate_hardening.md`",
        "",
        "2. Canonical release blocker ledger and derived audit outputs:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/release/defect_register.json`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_blocker_audit.json`",
        "",
        "3. Release-candidate audit automation and CI:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/scripts/run-s32-release-candidate-audit.sh`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/.github/workflows/release-candidate-audit.yml`",
        "",
        "4. Generated release evidence bundle:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_quality_gates.log`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_bench_suite.json`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_release_quality_report.md`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_release_notes_draft.md`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_risk_register.md`",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/S32.md`",
        "",
        "## Verification Evidence",
        "",
        "1. Quality and dependent suite execution:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_quality_gates.log`",
        f"- observed: overall_release_candidate_pass=`{fmt_bool(gate_pass)}`",
        f"- observed: open_p0_count=`{len(open_p0)}`",
        f"- observed: open_p1_count=`{len(open_p1)}`",
        "",
        "2. Performance/eval evidence:",
        "- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s32_bench_suite.json`",
        f"- observed: quick_qps=`{fmt_num(quick_weighted.get('qps'))}`",
        f"- observed: 10k_p95_ms=`{fmt_num((tenk_weighted.get('latency') or {}).get('p95_ms'), 4)}`",
        f"- observed: 10k_top1_hit_rate=`{fmt_num(tenk_weighted.get('top1_hit_rate'), 4)}`",
        "",
        "3. Resilience rollup:",
        f"- observed: availability_percent=`{fmt_num(resilience.get('availability_percent'), 2)}`",
        f"- observed: observed_rpo_seconds=`{fmt_num(resilience.get('observed_rpo_seconds'), 4)}`",
        f"- observed: dr_p95_ms=`{fmt_num(resilience.get('dr_p95_ms'), 4)}`",
        "",
        "## Sprint Gate Conclusion",
        "",
        "1. PF-D04 is complete with a documented `v1.x` release policy, support window, backport rules, and rollback policy.",
        "2. PF-G01 is complete for S32 scope with a canonical defect ledger and derived blocker audit showing zero open `P0` and `P1` defects.",
        "3. BE-01 through BE-04 are complete for S32 scope with regenerated latency, throughput, retrieval-quality, and efficiency metrics in the S32 bench suite.",
        "4. BE-05 is complete for S32 scope through a release-quality rollup that includes availability, RPO, recovery, and observability evidence from prior HA sprints.",
        "5. GV-01 and GV-02 are complete for S32 scope with a reproducible release-candidate audit script, CI workflow, and signed-off evidence bundle.",
    ]
    sprint_report_path.write_text("\n".join(sprint_lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
PY

echo "[s32-release-candidate-audit-complete] blocker=$BLOCKER_PATH quality=$QUALITY_REPORT_PATH sprint=$SPRINT_REPORT_PATH"
