# Release Candidate Hardening Runbook

Objective: produce a release-candidate evidence bundle for SQLRite `v1.0.0` with performance, quality, security, migration, and blocker-audit outputs generated from the current tree.

## Inputs

- repository root: `/Users/jameskaranja/Developer/projects/SQLRight`
- canonical defect ledger: `project_plan/release/defect_register.json`
- prior resilience artifacts:
  - `project_plan/reports/s17_benchmark_recovery.json`
  - `project_plan/reports/s18_benchmark_observability.json`
  - `project_plan/reports/s19_benchmark_dr_gate.json`
  - `project_plan/reports/s19_soak_slo_summary.json`

## Primary Command

```bash
bash scripts/run-s32-release-candidate-audit.sh
```

## What The Audit Runs

1. Formatting and test gates
- `cargo fmt --all --check`
- `cargo test`

2. Frozen interface and security gates
- `bash scripts/run-s26-api-compat-suite.sh`
- `bash scripts/run-s27-security-rbac-smoke.sh`
- `bash scripts/run-s28-security-audit-hardening.sh`

3. Migration and SQL-surface gates
- `bash scripts/run-s30-migration-suite.sh`
- `bash scripts/run-s31-sql-v2-and-api-migrations.sh`

4. Release benchmark/eval bundle
- `cargo run --bin sqlrite-bench-suite -- --profiles quick,10k --concurrency-profile quick --concurrency-levels 1,2,4`

## Generated Artifacts

- `project_plan/reports/s32_quality_gates.log`
- `project_plan/reports/s32_bench_suite.json`
- `project_plan/reports/s32_blocker_audit.json`
- `project_plan/reports/s32_release_quality_report.md`
- `project_plan/reports/s32_release_notes_draft.md`
- `project_plan/reports/s32_risk_register.md`
- `project_plan/reports/S32.md`

## Interpreting The Result

Release candidate is acceptable only if all are true:

1. `pass=true` in `project_plan/reports/s32_blocker_audit.json`
2. open `P0` count is `0`
3. open `P1` count is `0`
4. all dependent sprint suites pass
5. benchmark/eval summary exists for latency, throughput, retrieval quality, and efficiency

## Rollback Procedure

If the audit or post-audit smoke validation fails:

1. do not tag or publish the release
2. keep installers and packages pointed at the prior stable tag
3. restore from the latest validated snapshot if a migration or persistence check regressed
4. add the failure to `project_plan/release/defect_register.json` with severity and owner
5. rerun the full audit after the fix rather than manually editing any generated report

## Operator Notes

- Keep generated evidence in `project_plan/reports/`; do not overwrite prior sprint artifacts.
- Treat the defect ledger as source data and the blocker audit as a derived report.
- If a prior sprint artifact is missing, regenerate that sprint evidence before accepting the S32 gate.
